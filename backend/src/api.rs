use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::models::*;
use crate::streaming::StreamManager;

// ─── App State ──────────────────────────────────────────────────────────────

/// Shared application state passed to all handlers
#[derive(Clone)]
pub struct AppState {
    pub dvr_info: Arc<RwLock<Option<DvrInfo>>>,
    pub devices: Arc<RwLock<HashMap<u32, DiscoveredDevice>>>,
    pub channels: Arc<RwLock<Vec<Channel>>>,
    pub stream_manager: Arc<StreamManager>,
}

// ─── Router ─────────────────────────────────────────────────────────────────

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // DVR info
        .route("/api/dvr/info", get(get_dvr_info))
        // Device discovery
        .route("/api/devices", get(list_devices))
        .route("/api/devices/:id", get(get_device))
        // Channels
        .route("/api/channels", get(list_channels))
        // Stream management
        .route("/api/streams", get(list_streams))
        .route("/api/streams/:channel/start", post(start_stream))
        .route("/api/streams/:channel/stop", post(stop_stream))
        // WebSocket video stream
        .route("/ws/stream/:channel", get(ws_stream_handler))
        // Health check
        .route("/api/health", get(health_check))
        .with_state(state)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// GET /api/health
async fn health_check() -> Json<ApiResponse<String>> {
    Json(ApiResponse::ok("Neural DVR backend is running".to_string()))
}

/// GET /api/dvr/info
async fn get_dvr_info(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DvrInfo>>, StatusCode> {
    let info = state.dvr_info.read().await;
    match info.as_ref() {
        Some(dvr_info) => Ok(Json(ApiResponse::ok(dvr_info.clone()))),
        None => Ok(Json(ApiResponse::err(
            "DVR info not yet available. Discovery may still be running.",
        ))),
    }
}

/// GET /api/devices
async fn list_devices(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<DiscoveredDevice>>> {
    let devices = state.devices.read().await;
    let mut device_list: Vec<DiscoveredDevice> = devices.values().cloned().collect();
    device_list.sort_by_key(|d| d.channel_id);
    Json(ApiResponse::ok(device_list))
}

/// GET /api/devices/:id
async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<ApiResponse<DiscoveredDevice>>, StatusCode> {
    let devices = state.devices.read().await;
    match devices.get(&id) {
        Some(device) => Ok(Json(ApiResponse::ok(device.clone()))),
        None => Ok(Json(ApiResponse::err(format!(
            "Device with channel ID {} not found",
            id
        )))),
    }
}

/// GET /api/channels
async fn list_channels(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Channel>>> {
    let channels = state.channels.read().await;
    Json(ApiResponse::ok(channels.clone()))
}

/// GET /api/streams
async fn list_streams(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<StreamInfo>>> {
    let streams = state.stream_manager.list_streams().await;
    Json(ApiResponse::ok(streams))
}

/// POST /api/streams/:channel/start
async fn start_stream(
    State(state): State<AppState>,
    Path(channel_id): Path<u32>,
) -> Result<Json<ApiResponse<StreamInfo>>, StatusCode> {
    // Get channel name
    let channel_name = {
        let channels = state.channels.read().await;
        channels
            .iter()
            .find(|c| c.id == channel_id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("Channel {}", channel_id))
    };

    match state
        .stream_manager
        .start_stream(channel_id, &channel_name)
        .await
    {
        Ok(info) => Ok(Json(ApiResponse::ok(info))),
        Err(e) => {
            error!("Failed to start stream for channel {}: {}", channel_id, e);
            Ok(Json(ApiResponse::err(format!(
                "Failed to start stream: {}",
                e
            ))))
        }
    }
}

/// POST /api/streams/:channel/stop
async fn stop_stream(
    State(state): State<AppState>,
    Path(channel_id): Path<u32>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    match state.stream_manager.stop_stream(channel_id).await {
        Ok(_) => Ok(Json(ApiResponse::ok(format!(
            "Stream for channel {} stopped",
            channel_id
        )))),
        Err(e) => {
            error!("Failed to stop stream for channel {}: {}", channel_id, e);
            Ok(Json(ApiResponse::err(format!(
                "Failed to stop stream: {}",
                e
            ))))
        }
    }
}

/// GET /ws/stream/:channel — WebSocket endpoint for MJPEG streaming
async fn ws_stream_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(channel_id): Path<u32>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_stream(socket, state, channel_id))
}

async fn handle_ws_stream(mut socket: WebSocket, state: AppState, channel_id: u32) {
    info!("WebSocket client connected for channel {}", channel_id);

    // Subscribe to frame broadcast
    let rx = state.stream_manager.subscribe_frames(channel_id).await;

    let Some(mut rx) = rx else {
        warn!(
            "No active stream for channel {}, closing WebSocket",
            channel_id
        );
        let _ = socket
            .send(Message::Text(
                "{\"error\": \"Stream not started. Call POST /api/streams/{channel}/start first.\"}".into(),
            ))
            .await;
        return;
    };

    // Stream JPEG frames as binary WebSocket messages
    loop {
        match rx.recv().await {
            Ok(jpeg_data) => {
                if socket
                    .send(Message::Binary(jpeg_data.into()))
                    .await
                    .is_err()
                {
                    // Client disconnected
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                // Client is too slow, skip frames
                warn!(
                    "WebSocket client lagged {} frames for channel {}",
                    n, channel_id
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                // Stream ended
                info!("Stream ended for channel {}", channel_id);
                break;
            }
        }
    }

    info!("WebSocket client disconnected for channel {}", channel_id);
}
