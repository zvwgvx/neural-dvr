use anyhow::{Context, Result};
use reqwest::Client;
use tracing::{debug, error, info, warn};

use crate::config::AppConfig;
use crate::models::*;

/// Hikvision ISAPI client for communicating with DVR devices
#[derive(Clone)]
pub struct HikClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
}

impl HikClient {
    /// Create a new HikClient from application config
    pub fn new(config: &AppConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: config.isapi_base_url(),
            username: config.dvr_username.clone(),
            password: config.dvr_password.clone(),
        }
    }

    /// Make an authenticated GET request to ISAPI
    async fn get(&self, path: &str) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);
        debug!("ISAPI GET: {}", url);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .context(format!("Failed to connect to DVR at {}", url))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "ISAPI request failed with status {}: {}",
                status,
                body
            );
        }

        response
            .text()
            .await
            .context("Failed to read ISAPI response body")
    }

    /// Get DVR device information
    pub async fn get_device_info(&self) -> Result<DvrInfo> {
        info!("Fetching DVR device info...");

        let xml = self.get("/ISAPI/System/deviceInfo").await?;
        let parsed: IsapiDeviceInfo = quick_xml::de::from_str(&xml)
            .context("Failed to parse deviceInfo XML")?;

        Ok(DvrInfo {
            device_name: parsed.device_name.unwrap_or_else(|| "Unknown".into()),
            device_id: parsed.device_id.unwrap_or_default(),
            model: parsed.model.unwrap_or_else(|| "Unknown".into()),
            serial_number: parsed.serial_number.unwrap_or_default(),
            firmware_version: parsed.firmware_version.unwrap_or_default(),
            encoder_version: parsed.encoder_version.unwrap_or_default(),
            device_type: parsed.device_type.unwrap_or_else(|| "DVR".into()),
            channel_count: 0, // Will be updated by channel discovery
        })
    }

    /// Get list of video input channels
    pub async fn get_channels(&self) -> Result<Vec<Channel>> {
        info!("Fetching video input channels...");

        let xml = self
            .get("/ISAPI/System/Video/inputs/channels")
            .await?;

        let parsed: IsapiVideoInputChannelList = quick_xml::de::from_str(&xml)
            .context("Failed to parse video input channels XML")?;

        let channels: Vec<Channel> = parsed
            .channels
            .into_iter()
            .map(|ch| {
                let id = ch
                    .id
                    .as_deref()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);

                let enabled = ch
                    .video_input_enabled
                    .as_deref()
                    .map(|s| s == "true" || s == "1")
                    .unwrap_or(false);

                Channel {
                    id,
                    name: ch.name.unwrap_or_else(|| format!("Channel {}", id)),
                    enabled,
                    status: ChannelStatus::Unknown,
                    resolution_width: None,
                    resolution_height: None,
                    video_codec: None,
                }
            })
            .collect();

        info!("Found {} video input channels", channels.len());
        Ok(channels)
    }

    /// Get input proxy channels (camera devices connected to DVR)
    pub async fn get_input_proxy_channels(&self) -> Result<Vec<IsapiInputProxyChannel>> {
        info!("Fetching input proxy channels...");

        let xml = self
            .get("/ISAPI/ContentMgmt/InputProxy/channels")
            .await;

        match xml {
            Ok(xml_str) => {
                let parsed: IsapiInputProxyChannelList =
                    quick_xml::de::from_str(&xml_str)
                        .context("Failed to parse input proxy channels XML")?;
                info!(
                    "Found {} input proxy channels",
                    parsed.channels.len()
                );
                Ok(parsed.channels)
            }
            Err(e) => {
                warn!(
                    "Failed to fetch input proxy channels (may not be supported): {}",
                    e
                );
                Ok(vec![])
            }
        }
    }

    /// Check if a specific channel is online by trying to get its status
    pub async fn check_channel_status(&self, channel_id: u32) -> ChannelStatus {
        let path = format!(
            "/ISAPI/System/Video/inputs/channels/{}/status",
            channel_id
        );

        match self.get(&path).await {
            Ok(xml) => {
                // If we get a response, check if the channel reports online
                if xml.contains("online") || xml.contains("OK") || xml.contains("normal") {
                    ChannelStatus::Online
                } else if xml.contains("offline") || xml.contains("noVideo") {
                    ChannelStatus::Offline
                } else {
                    // Got a response but unclear status — treat as online
                    ChannelStatus::Online
                }
            }
            Err(e) => {
                debug!("Channel {} status check failed: {}", channel_id, e);
                ChannelStatus::Offline
            }
        }
    }

    /// Test connectivity to DVR
    pub async fn test_connection(&self) -> Result<bool> {
        info!("Testing connection to DVR at {}...", self.base_url);
        match self.get_device_info().await {
            Ok(info) => {
                info!(
                    "✓ Connected to DVR: {} ({})",
                    info.device_name, info.model
                );
                Ok(true)
            }
            Err(e) => {
                error!("✗ Failed to connect to DVR: {}", e);
                Ok(false)
            }
        }
    }
}
