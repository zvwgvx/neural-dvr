use anyhow::{Context, Result};

fn encode_rtsp_userinfo(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');

        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }

    encoded
}

/// Application configuration loaded from .env file
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Hikvision DVR host IP address
    pub dvr_host: String,
    /// Optional RTSP host override when it differs from DVR_HOST
    pub dvr_rtsp_host: String,
    /// Hikvision DVR ISAPI port (default: 80)
    pub dvr_port: u16,
    /// Hikvision DVR RTSP port (default: 554)
    pub dvr_rtsp_port: u16,
    /// DVR admin username
    pub dvr_username: String,
    /// DVR admin password
    pub dvr_password: String,
    /// Optional manual RTSP channels (e.g. 101,201,301)
    pub dvr_channels: Vec<u32>,
    /// Server bind host
    pub server_host: String,
    /// Server bind port
    pub server_port: u16,
}

impl AppConfig {
    /// Load configuration from .env file
    pub fn from_env() -> Result<Self> {
        // Support running either from `backend/` or repository root.
        dotenvy::from_filename_override(".env").ok();
        dotenvy::from_filename_override("backend/.env").ok();

        Ok(Self {
            dvr_host: std::env::var("DVR_HOST")
                .context("DVR_HOST must be set in .env")?,
            dvr_rtsp_host: std::env::var("DVR_RTSP_HOST")
                .unwrap_or_else(|_| {
                    std::env::var("DVR_HOST").unwrap_or_default()
                }),
            dvr_port: std::env::var("DVR_PORT")
                .unwrap_or_else(|_| "80".to_string())
                .parse()
                .context("DVR_PORT must be a valid port number")?,
            dvr_rtsp_port: std::env::var("DVR_RTSP_PORT")
                .unwrap_or_else(|_| "554".to_string())
                .parse()
                .context("DVR_RTSP_PORT must be a valid port number")?,
            dvr_username: std::env::var("DVR_USERNAME")
                .context("DVR_USERNAME must be set in .env")?,
            dvr_password: std::env::var("DVR_PASSWORD")
                .context("DVR_PASSWORD must be set in .env")?,
            dvr_channels: parse_dvr_channels(
                std::env::var("DVR_CHANNELS").ok(),
            )?,
            server_host: std::env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "3001".to_string())
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
        })
    }

    /// Build an RTSP URL for a given channel ID.
    ///
    /// If channel is already a full Hikvision channel ID (e.g. 101, 201),
    /// use it as-is. If a short channel index is provided (e.g. 1, 2),
    /// convert to main stream style (101, 201).
    pub fn rtsp_url(&self, channel: u32) -> String {
        let username = encode_rtsp_userinfo(&self.dvr_username);
        let password = encode_rtsp_userinfo(&self.dvr_password);
        let rtsp_channel = if channel >= 100 {
            channel
        } else {
            channel * 100 + 1
        };

        format!(
            "rtsp://{}:{}@{}:{}/Streaming/Channels/{}",
            username,
            password,
            self.dvr_rtsp_host,
            self.dvr_rtsp_port,
            rtsp_channel
        )
    }
}

fn parse_dvr_channels(raw: Option<String>) -> Result<Vec<u32>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut channels = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let channel = trimmed.parse::<u32>().with_context(|| {
            format!(
                "Invalid DVR_CHANNELS value '{}'. Expected comma-separated integers (e.g. 101,201,301)",
                trimmed
            )
        })?;
        channels.push(channel);
    }

    channels.sort_unstable();
    channels.dedup();
    Ok(channels)
}
