use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::hikvision::client::HikClient;
use crate::models::*;

/// Manages auto-discovery of cameras connected to the DVR
pub struct DeviceDiscovery {
    client: HikClient,
    config: AppConfig,
    /// Thread-safe map of discovered devices
    devices: Arc<RwLock<HashMap<u32, DiscoveredDevice>>>,
    /// Cached DVR info
    dvr_info: Arc<RwLock<Option<DvrInfo>>>,
    /// Cached channel list
    channels: Arc<RwLock<Vec<Channel>>>,
}

impl DeviceDiscovery {
    pub fn new(client: HikClient, config: AppConfig) -> Self {
        Self {
            client,
            config,
            devices: Arc::new(RwLock::new(HashMap::new())),
            dvr_info: Arc::new(RwLock::new(None)),
            channels: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get shared reference to devices map
    pub fn devices(&self) -> Arc<RwLock<HashMap<u32, DiscoveredDevice>>> {
        self.devices.clone()
    }

    /// Get shared reference to DVR info
    pub fn dvr_info(&self) -> Arc<RwLock<Option<DvrInfo>>> {
        self.dvr_info.clone()
    }

    /// Get shared reference to channels
    pub fn channels(&self) -> Arc<RwLock<Vec<Channel>>> {
        self.channels.clone()
    }

    async fn fallback_to_manual_channels(
        &self,
        reason: &str,
        dvr_info: Option<DvrInfo>,
    ) {
        if self.config.dvr_channels.is_empty() {
            return;
        }

        warn!(
            "Switching to manual channel fallback (DVR_CHANNELS) because: {}",
            reason
        );

        let now = Utc::now();
        let existing_devices = self.devices.read().await.clone();

        let fallback_channels: Vec<Channel> = self
            .config
            .dvr_channels
            .iter()
            .map(|channel_id| Channel {
                id: *channel_id,
                name: format!("Cam_{}", channel_id),
                enabled: true,
                status: ChannelStatus::Unknown,
                resolution_width: None,
                resolution_height: None,
                video_codec: None,
            })
            .collect();

        let mut fallback_devices = HashMap::new();
        for channel in &fallback_channels {
            let device = if let Some(existing) = existing_devices.get(&channel.id) {
                DiscoveredDevice {
                    id: existing.id,
                    channel_id: channel.id,
                    name: channel.name.clone(),
                    status: ChannelStatus::Unknown,
                    ip_address: existing.ip_address.clone(),
                    protocol: existing.protocol.clone(),
                    resolution: existing.resolution.clone(),
                    discovered_at: existing.discovered_at,
                    last_seen: now,
                }
            } else {
                DiscoveredDevice {
                    id: Uuid::new_v4(),
                    channel_id: channel.id,
                    name: channel.name.clone(),
                    status: ChannelStatus::Unknown,
                    ip_address: None,
                    protocol: Some("rtsp".to_string()),
                    resolution: None,
                    discovered_at: now,
                    last_seen: now,
                }
            };
            fallback_devices.insert(channel.id, device);
        }

        let mut info_value = dvr_info.unwrap_or_default();
        if info_value.device_name == "Unknown" {
            info_value.device_name = "Manual RTSP Mode".to_string();
        }
        info_value.channel_count = fallback_channels.len() as u32;

        *self.dvr_info.write().await = Some(info_value);
        *self.channels.write().await = fallback_channels;
        *self.devices.write().await = fallback_devices;

        info!(
            "Manual fallback active: {} channels from DVR_CHANNELS",
            self.config.dvr_channels.len()
        );
    }

    /// Run a single discovery cycle
    pub async fn discover_once(&self) {
        info!("🔍 Running device discovery cycle...");

        // 1. Fetch DVR info
        match self.client.get_device_info().await {
            Ok(mut info) => {
                // 2. Fetch channels
                match self.client.get_channels().await {
                    Ok(channels) => {
                        info.channel_count = channels.len() as u32;
                        *self.dvr_info.write().await = Some(info);

                        // 3. Check status of each channel
                        let mut updated_channels = Vec::new();
                        let mut devices = HashMap::new();

                        for mut channel in channels {
                            let status = self
                                .client
                                .check_channel_status(channel.id)
                                .await;
                            channel.status = status.clone();
                            debug!(
                                "Channel {} '{}': {}",
                                channel.id, channel.name, channel.status
                            );

                            // Create/update discovered device
                            let now = Utc::now();
                            let existing = self.devices.read().await;
                            let device = if let Some(existing_device) =
                                existing.get(&channel.id)
                            {
                                DiscoveredDevice {
                                    id: existing_device.id,
                                    channel_id: channel.id,
                                    name: channel.name.clone(),
                                    status: channel.status.clone(),
                                    ip_address: existing_device.ip_address.clone(),
                                    protocol: existing_device.protocol.clone(),
                                    resolution: None,
                                    discovered_at: existing_device.discovered_at,
                                    last_seen: now,
                                }
                            } else {
                                DiscoveredDevice {
                                    id: Uuid::new_v4(),
                                    channel_id: channel.id,
                                    name: channel.name.clone(),
                                    status: channel.status.clone(),
                                    ip_address: None,
                                    protocol: None,
                                    resolution: None,
                                    discovered_at: now,
                                    last_seen: now,
                                }
                            };
                            drop(existing);

                            devices.insert(channel.id, device);
                            updated_channels.push(channel);
                        }

                        // 4. Try to get input proxy channels for IP addresses
                        if let Ok(proxy_channels) =
                            self.client.get_input_proxy_channels().await
                        {
                            for proxy in proxy_channels {
                                if let Some(id_str) = &proxy.id {
                                    if let Ok(id) = id_str.parse::<u32>() {
                                        if let Some(device) = devices.get_mut(&id) {
                                            if let Some(ref source) = proxy.source_input {
                                                device.ip_address =
                                                    source.ip_address.clone();
                                                device.protocol =
                                                    source.protocol.clone();
                                            }
                                            if let Some(name) = &proxy.name {
                                                if !name.is_empty() {
                                                    device.name = name.clone();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let online_count = devices
                            .values()
                            .filter(|d| d.status == ChannelStatus::Online)
                            .count();
                        info!(
                            "✓ Discovery complete: {} devices found ({} online)",
                            devices.len(),
                            online_count
                        );

                        *self.devices.write().await = devices;
                        *self.channels.write().await = updated_channels;
                    }
                    Err(e) => {
                        error!("Failed to fetch channels: {}", e);
                        self.fallback_to_manual_channels(
                            &format!("failed to fetch channels: {}", e),
                            Some(info),
                        )
                        .await;
                    }
                }
            }
            Err(e) => {
                error!("Failed to fetch DVR info: {}", e);
                self.fallback_to_manual_channels(
                    &format!("failed to fetch DVR info: {}", e),
                    None,
                )
                .await;
            }
        }
    }

    /// Start the periodic discovery loop
    pub async fn start_discovery_loop(self: Arc<Self>) {
        let interval_secs = self.config.discovery_interval;
        info!(
            "Starting device discovery loop (interval: {}s)",
            interval_secs
        );

        // Run initial discovery immediately
        self.discover_once().await;

        // Then run periodically
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.tick().await; // Skip the first immediate tick

        loop {
            interval.tick().await;
            self.discover_once().await;
        }
    }
}
