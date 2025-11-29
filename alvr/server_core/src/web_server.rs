use crate::{
    ConnectionContext, FILESYSTEM_LAYOUT, SESSION_MANAGER, ServerCoreEvent,
    diagnostics::{self, DiagnosticsState},
    logging_backend::EVENTS_SENDER,
};
use alvr_common::{ConnectionState, LogEntry, anyhow::Result, error, info, log, parking_lot::Mutex};
use alvr_events::{ButtonEvent, EventType};
use alvr_packets::{ButtonEntry, ClientConnectionsAction, FirewallRulesAction, PathValuePair};
use alvr_session::SessionConfig;
use axum::{
    Json, Router,
    extract::{Request, State, WebSocketUpgrade, ws::Message},
    http::{
        HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    middleware,
    response::{Html, Response},
    routing,
};
use serde_json as json;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, sync::broadcast::error::RecvError};
use tower_http::{
    cors::{self, CorsLayer},
    set_header::SetResponseHeaderLayer,
};

// Diagnostics state - lazily initialized
static DIAGNOSTICS_STATE: std::sync::OnceLock<Arc<Mutex<DiagnosticsState>>> = std::sync::OnceLock::new();

fn get_diagnostics_state() -> Arc<Mutex<DiagnosticsState>> {
    DIAGNOSTICS_STATE.get_or_init(|| {
        let (state, _receiver) = DiagnosticsState::new();

        // Start background services
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            diagnostics::start_adb_status_polling(state_clone).await;
        });

        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            diagnostics::start_steamvr_log_tail(state_clone).await;
        });

        state
    }).clone()
}

const X_ALVR: &str = "X-ALVR";

// This is the actual core part of cors
// We require the X-ALVR header, but the browser forces a cors preflight
// if the site tries to send a request with it set since it's not-whitelisted
//
// The dashboard can just set the header and be allowed through without the preflight
// thus not getting blocked by allow_untrusted_http being disabled
async fn ensure_preflight(request: Request, next: middleware::Next) -> Response {
    if request.headers().contains_key(X_ALVR) || request.method() == Method::OPTIONS {
        next.run(request).await
    } else {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(format!("missing {X_ALVR} header").into())
            .unwrap()
    }
}

pub async fn web_server(connection_context: Arc<ConnectionContext>) -> Result<()> {
    let allow_untrusted_http;
    let web_server_port;

    {
        let session_manager = SESSION_MANAGER.read();
        allow_untrusted_http = session_manager.settings().connection.allow_untrusted_http;
        web_server_port = session_manager.settings().connection.web_server_port;
    }

    let mut cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([CONTENT_TYPE, X_ALVR.parse().unwrap()]);
    if allow_untrusted_http {
        cors = cors.allow_origin(cors::Any);
    }

    // API routes (require X-ALVR header)
    let api_router = Router::new()
        .route("/events", routing::get(events_websocket))
        .route("/log", routing::post(set_log))
        .nest(
            "/session",
            Router::new()
                .route("/", routing::get(get_session).post(update_session))
                .route("/values", routing::post(set_session_values))
                .route(
                    "/client-connections",
                    routing::post(update_client_connections),
                ),
        )
        .route("/buttons", routing::post(set_buttons))
        .route("/insert-idr", routing::post(insert_idr))
        .route("/capture-frame", routing::post(capture_frame))
        .nest(
            "/recording",
            Router::new()
                .route("/start", routing::post(start_recording))
                .route("/stop", routing::post(stop_recording)),
        )
        .nest(
            "/firewall-rules",
            Router::new()
                .route("/add", routing::post(add_firewall_rules))
                .route("/remove", routing::post(remove_firewall_rules)),
        )
        .nest(
            "/drivers",
            Router::new()
                .route("/", routing::get(get_driver_list))
                .route("/register-alvr", routing::post(register_alvr_driver))
                .route("/unregister", routing::post(unregister_driver)),
        )
        .nest(
            "/steamvr",
            Router::new()
                .route("/restart", routing::post(restart_steamvr))
                .route("/shutdown", routing::post(shutdown_steamvr)),
        )
        .route(
            "/version",
            routing::get(async || alvr_common::ALVR_VERSION.to_string()),
        )
        .route("/ping", routing::get(async || ()))
        // API diagnostics endpoints (for dashboard with X-ALVR header)
        .nest(
            "/diagnostics",
            Router::new()
                .route("/", routing::get(serve_diagnostics_ui))
                .route("/events", routing::get(diagnostics_websocket))
                .route("/status", routing::get(get_diagnostics_status))
                .route("/logcat/start", routing::post(start_logcat))
                .route("/logcat/stop", routing::post(stop_logcat)),
        )
        .layer(middleware::from_fn(ensure_preflight));

    // Diagnostics routes (no X-ALVR header required, for browser access)
    // Use permissive CORS to allow access from any host (e.g., fractal.local)
    let diagnostics_cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([CONTENT_TYPE])
        .allow_origin(cors::Any);

    let diagnostics_router = Router::new()
        .route("/", routing::get(serve_diagnostics_ui))
        .route("/ws", routing::get(diagnostics_websocket))
        .route("/status", routing::get(get_diagnostics_status))
        .route("/full", routing::get(get_diagnostics_full))
        .route("/logs", routing::get(get_diagnostics_logs))
        .route("/logcat/start", routing::post(start_logcat))
        .route("/logcat/stop", routing::post(stop_logcat))
        .layer(diagnostics_cors);

    let router = Router::new()
        .nest("/diagnostics", diagnostics_router)
        .nest("/api", api_router.layer(cors))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .with_state(connection_context);

    axum::serve(
        TcpListener::bind(SocketAddr::new([0, 0, 0, 0].into(), web_server_port))
            .await
            .unwrap(),
        router,
    )
    .await?;

    Ok(())
}

async fn events_websocket(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(async |mut ws| {
        let mut events_receiver = EVENTS_SENDER.subscribe();

        loop {
            match events_receiver.recv().await {
                Ok(event) => {
                    if let Err(e) = ws
                        .send(Message::Text(json::to_string(&event).unwrap().into()))
                        .await
                    {
                        info!("Failed to send event with websocket: {e}");
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => (),
                Err(RecvError::Closed) => break,
            }
        }
    })
}

async fn set_log(Json(entry): Json<LogEntry>) {
    let level = entry.severity.into_log_level();
    log::log!(level, "{}", entry.content);
}

async fn get_session() {
    alvr_events::send_event(EventType::Session(Box::new(
        crate::SESSION_MANAGER.read().session().clone(),
    )));
}

async fn update_session(Json(config): Json<SessionConfig>) {
    *SESSION_MANAGER.write().session_mut() = config;
}

async fn set_session_values(Json(descs): Json<Vec<PathValuePair>>) {
    SESSION_MANAGER.write().set_session_values(descs).ok();
}

async fn update_client_connections(
    State(ctx): State<Arc<ConnectionContext>>,
    Json((hostname, mut action)): Json<(String, ClientConnectionsAction)>,
) {
    let mut session_manager = SESSION_MANAGER.write();
    if matches!(action, ClientConnectionsAction::RemoveEntry)
        && let Some(entry) = session_manager.client_list().get(&hostname)
        && entry.connection_state != ConnectionState::Disconnected
    {
        ctx.clients_to_be_removed.lock().insert(hostname.clone());

        action = ClientConnectionsAction::SetConnectionState(ConnectionState::Disconnecting);
    }

    session_manager.update_client_connections(hostname, action);
}

async fn insert_idr(State(ctx): State<Arc<ConnectionContext>>) {
    ctx.events_sender.send(ServerCoreEvent::RequestIDR).ok();
}

async fn capture_frame(State(ctx): State<Arc<ConnectionContext>>) {
    ctx.events_sender.send(ServerCoreEvent::CaptureFrame).ok();
}

async fn start_recording(State(ctx): State<Arc<ConnectionContext>>) {
    crate::create_recording_file(&ctx, crate::SESSION_MANAGER.read().settings())
}

async fn stop_recording(State(ctx): State<Arc<ConnectionContext>>) {
    *ctx.video_recording_file.lock() = None;
}

async fn add_firewall_rules() {
    if let Err(e) =
        alvr_server_io::firewall_rules(FirewallRulesAction::Add, FILESYSTEM_LAYOUT.get().unwrap())
    {
        error!("Failed to add firewall rules! code: {e}");
    } else {
        info!("Successfully added firewall rules!");
    }
}

async fn remove_firewall_rules() {
    if let Err(e) = alvr_server_io::firewall_rules(
        FirewallRulesAction::Remove,
        FILESYSTEM_LAYOUT.get().unwrap(),
    ) {
        error!("Failed to remove firewall rules! code: {e}");
    } else {
        info!("Successfully removed firewall rules!");
    }
}

async fn get_driver_list() {
    if let Ok(list) = alvr_server_io::get_registered_drivers() {
        alvr_events::send_event(EventType::DriversList(list));
    }
}

async fn register_alvr_driver() {
    alvr_server_io::driver_registration(
        &[FILESYSTEM_LAYOUT
            .get()
            .unwrap()
            .openvr_driver_root_dir
            .clone()],
        true,
    )
    .ok();

    if let Ok(list) = alvr_server_io::get_registered_drivers() {
        alvr_events::send_event(EventType::DriversList(list));
    }
}

async fn unregister_driver(Json(path): Json<PathBuf>) {
    alvr_server_io::driver_registration(&[path], false).ok();

    if let Ok(list) = alvr_server_io::get_registered_drivers() {
        alvr_events::send_event(EventType::DriversList(list));
    }
}

async fn restart_steamvr(State(ctx): State<Arc<ConnectionContext>>) {
    ctx.events_sender.send(ServerCoreEvent::RestartPending).ok();
}

async fn shutdown_steamvr(State(ctx): State<Arc<ConnectionContext>>) {
    ctx.events_sender
        .send(ServerCoreEvent::ShutdownPending)
        .ok();
}

async fn set_buttons(
    State(ctx): State<Arc<ConnectionContext>>,
    Json(button_events): Json<Vec<ButtonEvent>>,
) {
    let button_entries = button_events
        .iter()
        .map(|b| ButtonEntry {
            path_id: alvr_common::hash_string(&b.path),
            value: b.value,
        })
        .collect();

    ctx.events_sender
        .send(ServerCoreEvent::Buttons(button_entries))
        .ok();
}

// ==================== Diagnostics Handlers ====================

/// Serve the diagnostics Web UI
async fn serve_diagnostics_ui() -> Html<&'static str> {
    Html(include_str!("diagnostics_ui.html"))
}

/// WebSocket endpoint for diagnostics events
async fn diagnostics_websocket(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(async |mut ws| {
        let state = get_diagnostics_state();
        let mut receiver = state.lock().subscribe();

        // Also subscribe to regular events (for streamer logs)
        let mut events_receiver = EVENTS_SENDER.subscribe();

        // Send initial status immediately so client has current state
        {
            let snapshot = diagnostics::get_diagnostics_snapshot(&state);
            // Send ADB status
            let adb_event = EventType::DiagAdbStatus(snapshot.adb_status);
            let _ = ws
                .send(Message::Text(json::to_string(&adb_event).unwrap().into()))
                .await;
            // Send logcat state
            let logcat_event = EventType::DiagLogcatState {
                active: snapshot.logcat_active,
            };
            let _ = ws
                .send(Message::Text(json::to_string(&logcat_event).unwrap().into()))
                .await;
        }

        loop {
            tokio::select! {
                // Handle diagnostics-specific events
                result = receiver.recv() => {
                    match result {
                        Ok(event) => {
                            let event_type: EventType = event.into();
                            if ws
                                .send(Message::Text(json::to_string(&event_type).unwrap().into()))
                                .await
                                .is_err()
                            {
                                break; // Client disconnected
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                // Forward events from main channel
                result = events_receiver.recv() => {
                    match result {
                        Ok(event) => {
                            // Convert/forward events to diagnostics websocket
                            let diag_event = match &event.event_type {
                                EventType::Log(entry) => {
                                    Some(EventType::DiagLog(alvr_events::DiagLogEntry {
                                        source: alvr_events::DiagSource::Streamer,
                                        severity: entry.severity,
                                        content: entry.content.clone(),
                                    }))
                                }
                                EventType::DebugGroup { group, message } => {
                                    Some(EventType::DiagLog(alvr_events::DiagLogEntry {
                                        source: alvr_events::DiagSource::Streamer,
                                        severity: alvr_common::LogSeverity::Debug,
                                        content: format!("[{}] {}", group, message),
                                    }))
                                }
                                // Forward diagnostic events directly
                                EventType::DiagLog(_)
                                | EventType::DiagAdbStatus(_)
                                | EventType::DiagLogcatState { .. } => {
                                    Some(event.event_type.clone())
                                }
                                _ => None,
                            };

                            if let Some(evt) = diag_event {
                                if ws
                                    .send(Message::Text(json::to_string(&evt).unwrap().into()))
                                    .await
                                    .is_err()
                                {
                                    break; // Client disconnected
                                }
                            }
                        }
                        Err(RecvError::Lagged(_)) => (),
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

/// Get current diagnostics status snapshot
async fn get_diagnostics_status() -> Json<diagnostics::DiagnosticsSnapshot> {
    let state = get_diagnostics_state();
    Json(diagnostics::get_diagnostics_snapshot(&state))
}

/// Get full diagnostics data including stored logs
async fn get_diagnostics_full() -> Json<diagnostics::DiagnosticsFull> {
    let state = get_diagnostics_state();
    Json(diagnostics::get_diagnostics_full(&state))
}

/// Get stored diagnostic logs
async fn get_diagnostics_logs() -> Json<Vec<diagnostics::StoredLogEntry>> {
    let state = get_diagnostics_state();
    Json(state.lock().get_stored_logs())
}

/// Start logcat streaming
async fn start_logcat() -> (StatusCode, String) {
    let state = get_diagnostics_state();
    match diagnostics::start_logcat(&state) {
        Ok(()) => (StatusCode::OK, "Logcat started".to_string()),
        Err(e) => (StatusCode::BAD_REQUEST, e),
    }
}

/// Stop logcat streaming
async fn stop_logcat() -> &'static str {
    let state = get_diagnostics_state();
    diagnostics::stop_logcat(&state);
    "Logcat stopped"
}
