'use client';

import { DiscoveredDevice } from '@/lib/api';

interface DeviceListProps {
    devices: DiscoveredDevice[];
    selectedChannel: number | null;
    onSelectChannel: (channelId: number) => void;
    loading: boolean;
}

export default function DeviceList({
    devices,
    selectedChannel,
    onSelectChannel,
    loading,
}: DeviceListProps) {
    const onlineCount = devices.filter(d => d.status === 'online').length;

    if (loading && devices.length === 0) {
        return (
            <div className="sidebar">
                <div className="sidebar-header">
                    <span className="sidebar-title">Devices</span>
                </div>
                <div className="loading-state">
                    <div className="spinner" />
                    <span style={{ fontSize: 13 }}>Discovering devices...</span>
                </div>
            </div>
        );
    }

    return (
        <div className="sidebar">
            <div className="sidebar-header">
                <span className="sidebar-title">Devices</span>
                <span className="device-count">{devices.length}</span>
            </div>

            {devices.length === 0 ? (
                <div className="empty-state">
                    <span>No devices found</span>
                    <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                        Check DVR connection
                    </span>
                </div>
            ) : (
                <div className="device-list">
                    {devices.map(device => (
                        <div
                            key={device.id}
                            className={`device-item ${selectedChannel === device.channelId ? 'active' : ''
                                }`}
                            onClick={() => onSelectChannel(device.channelId)}
                        >
                            <div className={`device-icon ${device.status}`} />
                            <div className="device-info">
                                <div className="device-name">{device.name}</div>
                                <div className="device-meta">
                                    <span
                                        className={`device-status ${device.status}`}
                                    >
                                        {device.status === 'online' ? 'Online' : 'Offline'}
                                    </span>
                                    {device.ipAddress && (
                                        <span>· {device.ipAddress}</span>
                                    )}
                                </div>
                            </div>
                        </div>
                    ))}
                </div>
            )}

            {/* Summary */}
            <div className="dvr-info-panel">
                <div className="dvr-info-title">Summary</div>
                <div className="dvr-info-row">
                    <span className="dvr-info-label">Total Channels</span>
                    <span className="dvr-info-value">{devices.length}</span>
                </div>
                <div className="dvr-info-row">
                    <span className="dvr-info-label">Online</span>
                    <span className="dvr-info-value">
                        {onlineCount}
                    </span>
                </div>
                <div className="dvr-info-row">
                    <span className="dvr-info-label">Offline</span>
                    <span className="dvr-info-value">
                        {devices.length - onlineCount}
                    </span>
                </div>
            </div>
        </div>
    );
}
