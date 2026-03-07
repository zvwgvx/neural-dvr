use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot, RwLock};
use tracing::{error, info, warn};

use crate::config::AppConfig;
use crate::models::*;

/// Manages RTSP capture streams and broadcasts JPEG frames via channels.
pub struct StreamManager {
    config: AppConfig,
    /// Active stream handles keyed by channel ID
    streams: Arc<RwLock<HashMap<u32, StreamHandle>>>,
}

struct StreamHandle {
    info: StreamInfo,
    /// Sender to broadcast JPEG frames to connected WebSocket clients
    frame_tx: broadcast::Sender<Vec<u8>>,
    /// Handle to abort the capture task
    abort_handle: tokio::task::JoinHandle<()>,
}

impl StreamManager {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            streams: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start streaming a channel via ffmpeg RTSP capture.
    pub async fn start_stream(
        &self,
        channel_id: u32,
        channel_name: &str,
    ) -> anyhow::Result<StreamInfo> {
        {
            let streams = self.streams.read().await;
            if let Some(handle) = streams.get(&channel_id) {
                return Ok(handle.info.clone());
            }
        }

        info!(
            "Starting ffmpeg stream for channel {} '{}'",
            channel_id, channel_name
        );

        let rtsp_url = self.config.rtsp_url(channel_id);
        let (frame_tx, _) = broadcast::channel::<Vec<u8>>(32);
        let frame_tx_clone = frame_tx.clone();

        let stream_info = StreamInfo {
            channel_id,
            channel_name: channel_name.to_string(),
            status: StreamStatus::Starting,
            ws_url: format!("/ws/stream/{}", channel_id),
            started_at: Some(Utc::now()),
            pid: None,
        };

        let streams = self.streams.clone();
        let ch_id = channel_id;
        let (first_frame_tx, first_frame_rx) = oneshot::channel();

        let abort_handle = tokio::spawn(async move {
            let tx = frame_tx_clone.clone();
            let streams_ref = streams.clone();
            let url = rtsp_url.clone();

            // Sentinel for signalling first-frame to the startup watcher (only used once).
            let mut first_tx = Some(first_frame_tx);
            let mut backoff = Duration::from_secs(2);

            loop {
                info!("Connecting ffmpeg for channel {}", ch_id);

                // Mark as Starting so the frontend knows we are reconnecting.
                {
                    let mut s = streams_ref.write().await;
                    if let Some(handle) = s.get_mut(&ch_id) {
                        handle.info.status = StreamStatus::Starting;
                    } else {
                        // StreamHandle was removed (stop_stream was called) — exit loop.
                        break;
                    }
                }

                let result = capture_rtsp_loop(
                    &url,
                    ch_id,
                    &tx,
                    first_tx.take(),
                ).await;

                match result {
                    Ok(()) => {
                        info!("ffmpeg capture for channel {} ended normally, restarting in {:?}", ch_id, backoff);
                    }
                    Err(e) => {
                        error!("ffmpeg capture error for channel {}: {}, restarting in {:?}", ch_id, e, backoff);
                    }
                }

                // Check if stream was explicitly stopped before sleeping.
                {
                    let s = streams_ref.read().await;
                    if !s.contains_key(&ch_id) {
                        info!("Stream for channel {} was stopped, not restarting", ch_id);
                        break;
                    }
                }

                tokio::time::sleep(backoff).await;

                // Exponential backoff, capped at 30 s.
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }

            // Only reaches here if stop_stream removed the handle.
            let mut s = streams_ref.write().await;
            if let Some(handle) = s.get_mut(&ch_id) {
                handle.info.status = StreamStatus::Stopped;
            }
        });

        let handle = StreamHandle {
            info: stream_info.clone(),
            frame_tx,
            abort_handle,
        };

        self.streams.write().await.insert(channel_id, handle);

        let streams = self.streams.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(std::time::Duration::from_secs(30), first_frame_rx)
                .await
            {
                Ok(Ok(())) => {
                    let mut s = streams.write().await;
                    if let Some(handle) = s.get_mut(&ch_id) {
                        if handle.info.status == StreamStatus::Starting {
                            handle.info.status = StreamStatus::Running;
                            info!("✓ ffmpeg stream running for channel {}", ch_id);
                        }
                    }
                }
                Ok(Err(_)) => {
                    let mut s = streams.write().await;
                    if let Some(handle) = s.get_mut(&ch_id) {
                        if handle.info.status == StreamStatus::Starting {
                            handle.info.status = StreamStatus::Error;
                        }
                    }
                    warn!(
                        "Channel {} closed before producing the first frame",
                        ch_id
                    );
                }
                Err(_) => {
                    let mut s = streams.write().await;
                    if let Some(handle) = s.get_mut(&ch_id) {
                        if handle.info.status == StreamStatus::Starting {
                            handle.info.status = StreamStatus::Error;
                        }
                    }
                    warn!(
                        "Channel {} did not produce a frame within the startup timeout",
                        ch_id
                    );
                }
            }
        });

        Ok(stream_info)
    }

    /// Subscribe to JPEG frames for a given channel.
    pub async fn subscribe_frames(
        &self,
        channel_id: u32,
    ) -> Option<broadcast::Receiver<Vec<u8>>> {
        let streams = self.streams.read().await;
        streams.get(&channel_id).map(|h| h.frame_tx.subscribe())
    }

    /// Stop streaming a channel.
    pub async fn stop_stream(&self, channel_id: u32) -> anyhow::Result<()> {
        info!("Stopping stream for channel {}", channel_id);

        let mut streams = self.streams.write().await;
        if let Some(handle) = streams.remove(&channel_id) {
            handle.abort_handle.abort();
        }

        info!("✓ Stopped stream for channel {}", channel_id);
        Ok(())
    }

    /// Stop all active streams.
    pub async fn stop_all(&self) {
        let channel_ids: Vec<u32> = self.streams.read().await.keys().cloned().collect();

        for channel_id in channel_ids {
            if let Err(e) = self.stop_stream(channel_id).await {
                error!("Failed to stop stream for channel {}: {}", channel_id, e);
            }
        }
    }

    /// Get list of all streams and their status.
    pub async fn list_streams(&self) -> Vec<StreamInfo> {
        self.streams
            .read()
            .await
            .values()
            .map(|h| h.info.clone())
            .collect()
    }
}

fn ffmpeg_transport_attempts() -> Vec<Option<String>> {
    let mut transports = if let Ok(raw) = std::env::var("FFMPEG_RTSP_TRANSPORT") {
        raw.split(',')
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| {
                if value.eq_ignore_ascii_case("auto") || value.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(value)
                }
            })
            .collect::<Vec<_>>()
    } else {
        vec![
            Some("tcp".to_string()),
            Some("udp".to_string()),
            None,
        ]
    };

    if transports.is_empty() {
        transports.push(None);
    }

    transports
}

fn ffmpeg_fps() -> String {
    std::env::var("FFMPEG_FPS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "15".to_string())
}

fn ffmpeg_quality() -> String {
    std::env::var("FFMPEG_QUALITY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "7".to_string())
}

fn ffmpeg_log_level() -> String {
    std::env::var("FFMPEG_LOGLEVEL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "info".to_string())
}

fn build_ffmpeg_command(
    rtsp_url: &str,
    transport: Option<&str>,
) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg(ffmpeg_log_level())
        .arg("-nostdin");

    if let Some(transport) = transport {
        command.arg("-rtsp_transport").arg(transport);

        if transport == "tcp" {
            command.arg("-rtsp_flags").arg("prefer_tcp");
        }
    }

    command
        .arg("-fflags")
        .arg("+nobuffer+discardcorrupt+genpts")
        .arg("-flags")
        .arg("low_delay")
        .arg("-analyzeduration")
        .arg("0")
        .arg("-probesize")
        .arg("32768")
        .arg("-i")
        .arg(rtsp_url)
        .arg("-an")
        .arg("-map")
        .arg("0:v:0")
        .arg("-vf")
        .arg(format!("fps=fps={}", ffmpeg_fps()))
        .arg("-c:v")
        .arg("mjpeg")
        .arg("-q:v")
        .arg(ffmpeg_quality())
        .arg("-f")
        .arg("mjpeg")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    command
}

fn extract_next_jpeg(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let start = buffer.windows(2).position(|w| w == [0xFF, 0xD8]);

    let Some(start) = start else {
        if buffer.len() > 1024 * 1024 {
            buffer.clear();
        }
        return None;
    };

    if start > 0 {
        buffer.drain(..start);
    }

    let end = buffer[2..]
        .windows(2)
        .position(|w| w == [0xFF, 0xD9])
        .map(|idx| idx + 4);

    let Some(end) = end else {
        if buffer.len() > 4 * 1024 * 1024 {
            buffer.clear();
        }
        return None;
    };

    Some(buffer.drain(..end).collect())
}

async fn log_ffmpeg_stderr(
    stderr: tokio::process::ChildStderr,
    channel_id: u32,
) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            info!("ffmpeg channel {}: {}", channel_id, trimmed);
        }
    }
}

/// Single ffmpeg session: tries all transports until one streams at least one frame,
/// then keeps reading until EOF/error. Returns Ok(()) on clean exit, Err on failure.
async fn capture_rtsp_loop(
    rtsp_url: &str,
    channel_id: u32,
    frame_tx: &broadcast::Sender<Vec<u8>>,
    mut first_frame_tx: Option<oneshot::Sender<()>>,
) -> anyhow::Result<()> {
    info!(
        "Opening RTSP stream for channel {}: {}",
        channel_id,
        rtsp_url.replace(&extract_password(rtsp_url), "****")
    );

    let mut last_error: Option<anyhow::Error> = None;
    let mut produced_first_frame = false;

    'transport: for transport in ffmpeg_transport_attempts() {
        let transport_label = transport.as_deref().unwrap_or("auto");
        info!(
            "Trying ffmpeg RTSP transport '{}' for channel {}",
            transport_label, channel_id
        );

        let mut command = build_ffmpeg_command(rtsp_url, transport.as_deref());
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                last_error = Some(anyhow::anyhow!(
                    "Failed to spawn ffmpeg for channel {} transport {}: {}",
                    channel_id, transport_label, e
                ));
                continue;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                last_error = Some(anyhow::anyhow!("ffmpeg stdout was not piped for channel {}", channel_id));
                continue;
            }
        };

        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(log_ffmpeg_stderr(stderr, channel_id));
        }

        let mut reader = BufReader::new(stdout);
        let mut read_buf = [0u8; 16 * 1024];
        let mut pending = Vec::with_capacity(64 * 1024);

        // Phase 1: read until the first frame arrives (to know the transport works).
        if !produced_first_frame {
            loop {
                let n = match reader.read(&mut read_buf).await {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = child.kill().await;
                        last_error = Some(anyhow::anyhow!("Read error for channel {} transport {}: {}", channel_id, transport_label, e));
                        continue 'transport;
                    }
                };

                if n == 0 {
                    let status = child.wait().await.ok();
                    let err = anyhow::anyhow!(
                        "ffmpeg exited before first frame for channel {} transport {}{}",
                        channel_id, transport_label,
                        status.map(|s| format!(" (status: {})", s)).unwrap_or_default()
                    );
                    warn!("{}", err);
                    last_error = Some(err);
                    continue 'transport;
                }

                pending.extend_from_slice(&read_buf[..n]);

                while let Some(jpeg_data) = extract_next_jpeg(&mut pending) {
                    produced_first_frame = true;
                    if let Some(tx) = first_frame_tx.take() {
                        let _ = tx.send(());
                        info!("First JPEG frame produced for channel {} ({} bytes)", channel_id, jpeg_data.len());
                    }
                    let _ = frame_tx.send(jpeg_data);
                }

                if produced_first_frame {
                    break;
                }
            }
        }

        // Phase 2: keep reading frames until EOF or error.
        loop {
            let n = match reader.read(&mut read_buf).await {
                Ok(n) => n,
                Err(e) => {
                    let _ = child.kill().await;
                    return Err(anyhow::anyhow!("Read error for channel {} transport {}: {}", channel_id, transport_label, e));
                }
            };

            if n == 0 {
                let status = child.wait().await.ok();
                return Err(anyhow::anyhow!(
                    "ffmpeg exited after streaming channel {} using transport {}{}",
                    channel_id, transport_label,
                    status.map(|s| format!(" (status: {})", s)).unwrap_or_default()
                ));
            }

            pending.extend_from_slice(&read_buf[..n]);

            while let Some(jpeg_data) = extract_next_jpeg(&mut pending) {
                let _ = frame_tx.send(jpeg_data);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!(
            "ffmpeg could not produce frames for channel {} with any configured transport",
            channel_id
        )
    }))
}

/// Extract password from RTSP URL for safe logging.
fn extract_password(url: &str) -> String {
    if let Some(start) = url.find(':') {
        if let Some(at) = url.rfind('@') {
            if let Some(pw_start) = url[start + 3..].find(':') {
                let pw_start = start + 3 + pw_start + 1;
                if pw_start < at {
                    return url[pw_start..at].to_string();
                }
            }
        }
    }
    String::new()
}

impl Drop for StreamManager {
    fn drop(&mut self) {
        info!("StreamManager dropped, streams will be cleaned up");
    }
}
