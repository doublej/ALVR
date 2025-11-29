pub mod commands;
mod parse;

use alvr_common::anyhow::Result;
use alvr_common::{dbg_connection, info, warn};
use alvr_session::WiredClientAutoLaunchConfig;
use alvr_system_info::{
    ClientFlavor, PACKAGE_NAME_GITHUB_DEV, PACKAGE_NAME_GITHUB_STABLE, PACKAGE_NAME_STORE,
};
use parse::ConnectionState;
use std::collections::HashSet;
use std::time::Duration;

pub enum WiredConnectionStatus {
    Ready,
    NotReady(String),
}

pub struct WiredConnection {
    adb_path: String,
}

impl WiredConnection {
    pub fn new(
        layout: &alvr_filesystem::Layout,
        download_progress_callback: impl Fn(usize, Option<usize>),
    ) -> Result<Self> {
        let adb_path = commands::require_adb(layout, download_progress_callback)?;

        Ok(Self { adb_path })
    }

    pub fn setup(
        &self,
        control_port: u16,
        stream_port: u16,
        client_type: &ClientFlavor,
        client_autolaunch: Option<WiredClientAutoLaunchConfig>,
    ) -> Result<WiredConnectionStatus> {
        let devices = commands::list_devices(&self.adb_path)?;

        dbg_connection!("wired_connection: Found {} device(s)", devices.len());
        for device in &devices {
            dbg_connection!(
                "wired_connection: Device serial={:?}, state={:?}",
                device.serial,
                device.connection_state
            );
        }

        let device = devices.into_iter().find(|d| {
            d.serial
                .as_ref()
                .map(|s| !s.starts_with("127.0.0.1"))
                .unwrap_or(false)
        });

        let Some(device) = device else {
            return Ok(WiredConnectionStatus::NotReady(
                "No wired devices found".to_owned(),
            ));
        };

        let Some(device_serial) = device.serial else {
            return Ok(WiredConnectionStatus::NotReady(
                "Device has no serial number".to_owned(),
            ));
        };

        match device.connection_state {
            Some(ConnectionState::Device) => {
                dbg_connection!(
                    "wired_connection: Device {} is in Device state",
                    device_serial
                );
            }
            Some(ConnectionState::Unauthorized) => {
                return Ok(WiredConnectionStatus::NotReady(
                    "Device is unauthorized. Please accept USB debugging on the headset".to_owned(),
                ));
            }
            Some(ConnectionState::Offline) => {
                return Ok(WiredConnectionStatus::NotReady(
                    "Device is offline".to_owned(),
                ));
            }
            Some(ConnectionState::NoPermissions) => {
                return Ok(WiredConnectionStatus::NotReady(
                    "No permissions to access device. Check USB permissions".to_owned(),
                ));
            }
            Some(ref state) => {
                return Ok(WiredConnectionStatus::NotReady(format!(
                    "Device is in unsupported state: {:?}",
                    state
                )));
            }
            None => {
                return Ok(WiredConnectionStatus::NotReady(
                    "Device state unknown".to_owned(),
                ));
            }
        };

        let ports = HashSet::from([control_port, stream_port]);

        // Clear stale port forwards before setting up new ones
        dbg_connection!(
            "wired_connection: Clearing stale port forwards for device {}",
            device_serial
        );
        if let Err(e) = commands::remove_all_forwards(&self.adb_path, &device_serial) {
            warn!(
                "wired_connection: Failed to remove stale forwards for device {}: {}",
                device_serial, e
            );
        }

        // Set up port forwarding
        for port in &ports {
            dbg_connection!(
                "wired_connection: Setting up port forward {} for device {}",
                port,
                device_serial
            );
            commands::forward_port(&self.adb_path, &device_serial, *port)?;
        }

        // Verify port forwarding was successful
        let forwarded_ports: HashSet<u16> =
            commands::list_forwarded_ports(&self.adb_path, &device_serial)?
                .into_iter()
                .map(|f| f.local)
                .collect();

        for port in &ports {
            if !forwarded_ports.contains(port) {
                warn!(
                    "wired_connection: Port forward verification failed for port {}",
                    port
                );
                return Ok(WiredConnectionStatus::NotReady(format!(
                    "Failed to verify port forwarding for port {}",
                    port
                )));
            }
        }

        dbg_connection!(
            "wired_connection: Port forwarding verified for device {}",
            device_serial
        );

        let Some(process_name) = get_process_name(&self.adb_path, &device_serial, client_type)
        else {
            return Ok(WiredConnectionStatus::NotReady(
                "No suitable ALVR client is installed".to_owned(),
            ));
        };

        dbg_connection!(
            "wired_connection: Checking client state for process {}",
            process_name
        );

        // Check if process is running
        let process_id = commands::get_process_id(&self.adb_path, &device_serial, &process_name)?;
        dbg_connection!(
            "wired_connection: Process ID check result: {:?}",
            process_id
        );

        if process_id.is_none() {
            if let Some(client_autolaunch) = client_autolaunch {
                if client_autolaunch.boot_delay > 0 {
                    match commands::get_uptime(&self.adb_path, &device_serial) {
                        Ok(uptime) => {
                            dbg_connection!(
                                "wired_connection: Device uptime: {:?}, boot delay: {}s",
                                uptime,
                                client_autolaunch.boot_delay
                            );
                            if uptime < Duration::from_secs(client_autolaunch.boot_delay.into()) {
                                return Ok(WiredConnectionStatus::NotReady(
                                    "Waiting for device boot".to_owned(),
                                ));
                            }
                        }
                        Err(failure) => {
                            warn!("wired_connection: get_uptime failed with {}", failure);
                        }
                    }
                }

                dbg_connection!(
                    "wired_connection: Starting ALVR client application {}",
                    process_name
                );
                commands::start_application(&self.adb_path, &device_serial, &process_name)?;
                Ok(WiredConnectionStatus::NotReady(
                    "Starting ALVR client".to_owned(),
                ))
            } else {
                Ok(WiredConnectionStatus::NotReady(
                    "ALVR client is not running".to_owned(),
                ))
            }
        } else {
            // Check if activity is resumed
            let is_resumed =
                commands::is_activity_resumed(&self.adb_path, &device_serial, &process_name)?;
            dbg_connection!(
                "wired_connection: Activity resumed check result: {}",
                is_resumed
            );

            if !is_resumed {
                return Ok(WiredConnectionStatus::NotReady(
                    "ALVR client is paused".to_owned(),
                ));
            }

            // Final verification: re-check device state before declaring ready
            // This helps catch race conditions where device state changed during setup
            dbg_connection!(
                "wired_connection: Performing final device state verification for {}",
                device_serial
            );
            let final_devices = commands::list_devices(&self.adb_path)?;
            let final_device = final_devices
                .into_iter()
                .find(|d| d.serial.as_ref() == Some(&device_serial));

            match final_device {
                Some(device) if device.connection_state == Some(ConnectionState::Device) => {
                    info!(
                        "wired_connection: Connection ready for device {} with client {}",
                        device_serial, process_name
                    );
                    Ok(WiredConnectionStatus::Ready)
                }
                Some(device) => {
                    warn!(
                        "wired_connection: Device state changed during setup: {:?}",
                        device.connection_state
                    );
                    Ok(WiredConnectionStatus::NotReady(format!(
                        "Device state changed during setup: {:?}",
                        device.connection_state
                    )))
                }
                None => {
                    warn!(
                        "wired_connection: Device {} disappeared during setup",
                        device_serial
                    );
                    Ok(WiredConnectionStatus::NotReady(
                        "Device disconnected during setup".to_owned(),
                    ))
                }
            }
        }
    }

    /// Explicitly shut down the ADB server.
    /// Normally the ADB server is left running for faster reconnection.
    /// Call this method only when you need to clean up the ADB server.
    pub fn shutdown(&self) -> Result<()> {
        info!("wired_connection: Explicitly shutting down ADB server");
        commands::kill_server(&self.adb_path)?;
        Ok(())
    }
}

impl Drop for WiredConnection {
    fn drop(&mut self) {
        // Don't kill ADB server on drop - let it persist for faster reconnection.
        // Users can call shutdown() explicitly if they need to clean up.
        dbg_connection!("wired_connection: Dropping WiredConnection (ADB server left running)");
    }
}

pub fn get_process_name(
    adb_path: &str,
    device_serial: &str,
    flavor: &ClientFlavor,
) -> Option<String> {
    let flavor_str = match flavor {
        ClientFlavor::Store => "Store",
        ClientFlavor::Github => "Github",
        ClientFlavor::Custom(name) => name.as_str(),
    };
    dbg_connection!(
        "wired_connection: Looking for client with flavor: {}",
        flavor_str
    );

    let fallbacks = match flavor {
        ClientFlavor::Store => {
            if alvr_common::is_stable() {
                vec![PACKAGE_NAME_STORE, PACKAGE_NAME_GITHUB_STABLE]
            } else {
                vec![PACKAGE_NAME_GITHUB_DEV]
            }
        }
        ClientFlavor::Github => {
            if alvr_common::is_stable() {
                vec![PACKAGE_NAME_GITHUB_STABLE, PACKAGE_NAME_STORE]
            } else {
                vec![PACKAGE_NAME_GITHUB_DEV]
            }
        }
        ClientFlavor::Custom(name) => {
            if alvr_common::is_stable() {
                vec![name, PACKAGE_NAME_STORE, PACKAGE_NAME_GITHUB_STABLE]
            } else {
                vec![name, PACKAGE_NAME_GITHUB_DEV]
            }
        }
    };

    // Check each fallback package in order
    for package_name in &fallbacks {
        dbg_connection!("wired_connection: Checking for package: {}", package_name);
        match commands::is_package_installed(adb_path, device_serial, package_name) {
            Ok(true) => {
                dbg_connection!("wired_connection: Found installed package: {}", package_name);
                return Some((*package_name).to_string());
            }
            Ok(false) => {
                dbg_connection!(
                    "wired_connection: Package {} not installed",
                    package_name
                );
            }
            Err(e) => {
                warn!(
                    "wired_connection: Error checking package {}: {}",
                    package_name, e
                );
            }
        }
    }

    // No matching client found - check if other client types are installed
    dbg_connection!("wired_connection: No matching client found, checking for mismatches");

    let all_packages = [
        PACKAGE_NAME_STORE,
        PACKAGE_NAME_GITHUB_STABLE,
        PACKAGE_NAME_GITHUB_DEV,
    ];

    let installed_packages: Vec<&str> = all_packages
        .iter()
        .filter(|pkg| {
            commands::is_package_installed(adb_path, device_serial, pkg)
                .unwrap_or(false)
        })
        .copied()
        .collect();

    if !installed_packages.is_empty() {
        warn!(
            "wired_connection: Client type mismatch detected! \
            You have installed: {:?}, but your 'Wired Client Type' setting is configured for: {}. \
            Please change the 'Wired Client Type' setting in the dashboard to match the installed client.",
            installed_packages, flavor_str
        );
    } else {
        dbg_connection!("wired_connection: No ALVR client packages found on device");
    }

    None
}
