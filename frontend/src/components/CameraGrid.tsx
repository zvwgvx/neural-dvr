'use client';

import { DiscoveredDevice, StreamInfo } from '@/lib/api';
import VideoPlayer from './VideoPlayer';
import { getWsStreamUrl } from '@/lib/api';
import { useState } from 'react';

interface CameraGridProps {
    devices: DiscoveredDevice[];
    streams: Map<number, StreamInfo>;
    loadingChannels: Set<number>;
}

export default function CameraGrid({ devices, streams, loadingChannels }: CameraGridProps) {
    // null = 2x3 grid, number = spotlight on that channelId
    const [spotlight, setSpotlight] = useState<number | null>(null);

    const handleDoubleClick = (channelId: number) => {
        setSpotlight(prev => (prev === channelId ? null : channelId));
    };

    return (
        <div className="camera-grid-root">
            {devices.length === 0 ? (
                <div className="empty-state">
                    <span>No cameras discovered yet</span>
                    <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                        Waiting for device discovery...
                    </span>
                </div>
            ) : (
                <div className={`camera-grid ${spotlight !== null ? 'grid-spotlight' : 'grid-2x3'}`}>
                    {devices.map(device => {
                        const stream = streams.get(device.channelId);
                        const isStreaming = stream?.status === 'running' || stream?.status === 'starting';
                        const isLoading = loadingChannels.has(device.channelId);
                        const isSpotlit = spotlight === device.channelId;
                        const isHidden = spotlight !== null && !isSpotlit;

                        return (
                            <VideoPlayer
                                key={device.channelId}
                                channelId={device.channelId}
                                channelName={device.name}
                                wsUrl={getWsStreamUrl(device.channelId)}
                                isStreaming={isStreaming}
                                streamStatus={stream?.status}
                                loading={isLoading}
                                hidden={isHidden}
                                onDoubleClick={() => handleDoubleClick(device.channelId)}
                            />
                        );
                    })}
                </div>
            )}
        </div>
    );
}
