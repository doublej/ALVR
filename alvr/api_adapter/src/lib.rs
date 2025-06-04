use std::{io::ErrorKind, net::{TcpStream, SocketAddr}, str::FromStr, time::Duration};

use alvr_events::{Event, EventType};
use alvr_packets::{ServerRequest, PathValuePair};
use serde_json::Value;
use urlencoding::{encode, decode};
use alvr_session::{SessionConfig, Settings};
use tungstenite::{client::IntoClientRequest, Message};

/// Simple API adapter to communicate with an ALVR server instance.
///
/// Requests are sent over HTTP and responses are received through the
/// `/api/events` websocket, mirroring the behaviour of the dashboard.
#[derive(Clone)]
pub struct ApiAdapter {
    port: u16,
    http: ureq::Agent,
}

impl ApiAdapter {
    /// Create a new adapter targeting the given server port.
    pub fn new(port: u16) -> Self {
        let http = ureq::Agent::builder()
            .timeout(Duration::from_millis(500))
            .build();

        Self { port, http }
    }

    fn request(&self, request: &ServerRequest) -> Result<(), String> {
        let uri = format!("http://127.0.0.1:{}/api/dashboard-request", self.port);
        self
            .http
            .post(&uri)
            .set("X-ALVR", "true")
            .send_json(request)
            .map_err(|e| e.to_string())?
            .into_string();

        Ok(())
    }

    fn wait_for_event<F, T>(&self, mut filter: F) -> Result<T, String>
    where
        F: FnMut(EventType) -> Option<T>,
    {
        let uri = format!("ws://127.0.0.1:{}/api/events", self.port);

        let socket = TcpStream::connect_timeout(
            &SocketAddr::from_str(&format!("127.0.0.1:{}", self.port)).unwrap(),
            Duration::from_secs(1),
        )
        .map_err(|e| e.to_string())?;

        let mut req = uri
            .into_client_request()
            .map_err(|e| e.to_string())?;
        req.headers_mut()
            .insert("X-ALVR", "true".parse().unwrap());

        let (mut ws, _) = tungstenite::client(req, socket).map_err(|e| e.to_string())?;
        ws.get_mut().set_read_timeout(Some(Duration::from_secs(2))).ok();

        loop {
            match ws.read_message() {
                Ok(Message::Text(text)) => {
                    if let Ok(event) = serde_json::from_str::<Event>(&text) {
                        if let Some(res) = filter(event.event_type) {
                            return Ok(res);
                        }
                    }
                }
                Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => return Err(e.to_string()),
                _ => {}
            }
        }
    }

    /// Fetch the current session configuration from the server.
    pub fn get_session(&self) -> Result<SessionConfig, String> {
        self.request(&ServerRequest::GetSession)?;
        self.wait_for_event(|e| match e {
            EventType::Session(session) => Some(*session),
            _ => None,
        })
    }

    /// Overwrite the session configuration on the server.
    pub fn update_session(&self, session: &SessionConfig) -> Result<(), String> {
        self.request(&ServerRequest::UpdateSession(Box::new(session.clone())))
    }

    /// Modify a subset of session values using a list of path/value pairs.
    pub fn set_values(&self, descs: Vec<PathValuePair>) -> Result<(), String> {
        self.request(&ServerRequest::SetValues(descs))
    }

    /// Convenience method returning just the `Settings` structure from the server.
    pub fn get_settings(&self) -> Result<Settings, String> {
        Ok(self.get_session()?.to_settings())
    }

    /// Retrieve a single setting value using a dotted path.
    pub fn get_value(&self, path: &str) -> Result<Value, String> {
        let uri = format!(
            "http://127.0.0.1:{}/api/settings?path={}",
            self.port,
            encode(path)
        );

        let resp = self
            .http
            .get(&uri)
            .set("X-ALVR", "true")
            .call()
            .map_err(|e| e.to_string())?;

        let reader = resp.into_reader();
        serde_json::from_reader(reader).map_err(|e| e.to_string())
    }

    /// Set a single setting value using a dotted path.
    pub fn set_value(&self, path: &str, value: &Value) -> Result<(), String> {
        let uri = format!(
            "http://127.0.0.1:{}/api/settings?path={}",
            self.port,
            encode(path)
        );

        self
            .http
            .post(&uri)
            .set("X-ALVR", "true")
            .send_json(value)
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

