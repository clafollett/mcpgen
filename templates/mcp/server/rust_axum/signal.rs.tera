//! Async signal handling for hot reload and graceful shutdown.
//
// Handles SIGHUP (reload config/env) and SIGTERM/SIGINT (graceful shutdown)
// using idiomatic async Rust patterns with Tokio and signal-hook.
use tokio::signal::unix::{signal, SignalKind};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::info;

/// Represents a signal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalEvent {
    Reload,
    Shutdown,
}

/// Spawns an async signal listener.
///
/// - On SIGHUP: notifies with `SignalEvent::Reload`
/// - On SIGTERM/SIGINT: notifies with `SignalEvent::Shutdown`
///
/// # Arguments
/// * `notify`: An `Arc<Notify>` used to trigger reload/shutdown logic elsewhere in your app.
/// * `event`: A `tokio::sync::Mutex<Option<SignalEvent>>` to communicate the event type.
pub async fn spawn_signal_listener(notify: Arc<Notify>, event: Arc<Mutex<Option<SignalEvent>>>) {
    // Create Unix signal streams for SIGHUP, SIGTERM, and SIGINT
    let mut sighup = signal(SignalKind::hangup()).expect("Failed to register SIGHUP");
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT");
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = sighup.recv() => {
                    info!(target: "signal", "Received SIGHUP: triggering config reload");
                    let mut ev = event.lock().await;
                    *ev = Some(SignalEvent::Reload);
                    notify.notify_one();
                }
                _ = sigterm.recv() => {
                    info!(target: "signal", "Received SIGTERM: triggering graceful shutdown");
                    let mut ev = event.lock().await;
                    *ev = Some(SignalEvent::Shutdown);
                    notify.notify_one();
                }
                _ = sigint.recv() => {
                    info!(target: "signal", "Received SIGINT: triggering graceful shutdown");
                    let mut ev = event.lock().await;
                    *ev = Some(SignalEvent::Shutdown);
                    notify.notify_one();
                }
            }
        }
    });
}
