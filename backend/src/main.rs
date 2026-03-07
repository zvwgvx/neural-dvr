mod api;
mod config;
mod models;
mod streaming;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

use api::{AppState, create_router};
use config::AppConfig;
use models::{Channel, ChannelStatus, DiscoveredDevice, DvrInfo};
use streaming::StreamManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "neural_dvr=info,tower_http=info".into()),
        )
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .init();

    info!("┌─────────────────────────────────────────┐");
    info!("│        Neural DVR — Hikvision DVR       │");
    info!("│      FFmpeg RTSP Streaming Backend      │");
    info!("└─────────────────────────────────────────┘");

    // Load configuration
    let config = AppConfig::from_env()?;
    info!(
        "DVR target: {}:{} (RTSP host: {}:{})",
        config.dvr_host, config.dvr_port, config.dvr_rtsp_host, config.dvr_rtsp_port
    );
    if !config.dvr_channels.is_empty() {
        info!("Manual RTSP channels configured: {:?}", config.dvr_channels);
    }
    info!(
        "Server will listen on {}:{}",
        config.server_host, config.server_port
    );

    if config.dvr_channels.is_empty() {
        anyhow::bail!(
            "DVR_CHANNELS is empty. Set it in backend/.env, e.g. DVR_CHANNELS=101,201,301"
        );
    }

    info!("Starting in manual RTSP mode (ISAPI disabled)");

    let channels: Vec<Channel> = config
        .dvr_channels
        .iter()
        .map(|channel_id| Channel {
            id: *channel_id,
            name: format!("Cam_{}", channel_id),
            enabled: true,
            status: ChannelStatus::Online,
            resolution_width: None,
            resolution_height: None,
            video_codec: None,
        })
        .collect();

    let now = Utc::now();
    let devices_map: HashMap<u32, DiscoveredDevice> = channels
        .iter()
        .map(|channel| {
            (
                channel.id,
                DiscoveredDevice {
                    id: Uuid::new_v4(),
                    channel_id: channel.id,
                    name: channel.name.clone(),
                    status: ChannelStatus::Online,
                    ip_address: Some(config.dvr_host.clone()),
                    protocol: Some("rtsp".to_string()),
                    resolution: None,
                    discovered_at: now,
                    last_seen: now,
                },
            )
        })
        .collect();

    let dvr_info = DvrInfo {
        device_name: "Manual RTSP".to_string(),
        device_id: config.dvr_host.clone(),
        model: "Hikvision".to_string(),
        serial_number: String::new(),
        firmware_version: String::new(),
        encoder_version: String::new(),
        device_type: "DVR".to_string(),
        channel_count: channels.len() as u32,
    };

    // Initialize stream manager
    let stream_manager = Arc::new(StreamManager::new(config.clone()));

    // Build app state
    let app_state = AppState {
        dvr_info: Arc::new(RwLock::new(Some(dvr_info))),
        devices: Arc::new(RwLock::new(devices_map)),
        channels: Arc::new(RwLock::new(channels)),
        stream_manager: stream_manager.clone(),
    };

    // CORS layer for Next.js frontend
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the main router
    let app = create_router(app_state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start HTTP server
    let addr: SocketAddr = format!("{}:{}", config.server_host, config.server_port)
        .parse()
        .expect("Invalid server address");

    info!("🚀 Server starting on http://{}", addr);
    info!("   API:       http://{}/api/health", addr);
    info!("   WebSocket: ws://{}/ws/stream/{{channel}}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(stream_manager))
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal(stream_manager: Arc<StreamManager>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, stopping all streams...");
    stream_manager.stop_all().await;
}
