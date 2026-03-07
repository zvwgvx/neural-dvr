use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── DVR Info ────────────────────────────────────────────────────────────────

/// DVR device information from ISAPI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DvrInfo {
    pub device_name: String,
    pub device_id: String,
    pub model: String,
    pub serial_number: String,
    pub firmware_version: String,
    pub encoder_version: String,
    pub device_type: String,
    pub channel_count: u32,
}

impl Default for DvrInfo {
    fn default() -> Self {
        Self {
            device_name: "Unknown".to_string(),
            device_id: String::new(),
            model: "Unknown".to_string(),
            serial_number: String::new(),
            firmware_version: String::new(),
            encoder_version: String::new(),
            device_type: "DVR".to_string(),
            channel_count: 0,
        }
    }
}

// ─── Channel ────────────────────────────────────────────────────────────────

/// Video input channel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
    pub status: ChannelStatus,
    pub resolution_width: Option<u32>,
    pub resolution_height: Option<u32>,
    pub video_codec: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelStatus {
    Online,
    Offline,
    Unknown,
}

impl std::fmt::Display for ChannelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelStatus::Online => write!(f, "online"),
            ChannelStatus::Offline => write!(f, "offline"),
            ChannelStatus::Unknown => write!(f, "unknown"),
        }
    }
}

// ─── Discovered Device ──────────────────────────────────────────────────────

/// A camera device discovered on the DVR
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDevice {
    pub id: Uuid,
    pub channel_id: u32,
    pub name: String,
    pub status: ChannelStatus,
    pub ip_address: Option<String>,
    pub protocol: Option<String>,
    pub resolution: Option<String>,
    pub discovered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

// ─── Stream Info ────────────────────────────────────────────────────────────

/// Information about an active stream
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub channel_id: u32,
    pub channel_name: String,
    pub status: StreamStatus,
    pub ws_url: String,
    pub started_at: Option<DateTime<Utc>>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StreamStatus {
    Starting,
    Running,
    Stopped,
    Error,
}

impl std::fmt::Display for StreamStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamStatus::Starting => write!(f, "starting"),
            StreamStatus::Running => write!(f, "running"),
            StreamStatus::Stopped => write!(f, "stopped"),
            StreamStatus::Error => write!(f, "error"),
        }
    }
}

// ─── API Responses ──────────────────────────────────────────────────────────

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl ToString) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.to_string()),
        }
    }
}
