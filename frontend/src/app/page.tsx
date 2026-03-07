'use client';

import { useEffect, useState, useCallback, useRef } from 'react';
import {
  type DiscoveredDevice,
  type StreamInfo,
  getDevices,
  getStreams,
  startStream,
  healthCheck,
} from '@/lib/api';
import CameraGrid from '@/components/CameraGrid';

export default function DashboardPage() {
  const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
  const [streams, setStreams] = useState<Map<number, StreamInfo>>(new Map());
  const [connectionStatus, setConnectionStatus] = useState<'connecting' | 'online' | 'offline'>('connecting');
  const [loadingChannels, setLoadingChannels] = useState<Set<number>>(new Set());
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const isOnline = await healthCheck();
      setConnectionStatus(isOnline ? 'online' : 'offline');
      if (!isOnline) return;

      const [devicesRes, streamsRes] = await Promise.all([getDevices(), getStreams()]);

      if (devicesRes.success && devicesRes.data) setDevices(devicesRes.data);
      if (streamsRes.success && streamsRes.data) {
        const streamMap = new Map<number, StreamInfo>();
        streamsRes.data.forEach(s => streamMap.set(s.channelId, s));
        setStreams(streamMap);
      }
    } catch {
      setConnectionStatus('offline');
    }
  }, []);

  useEffect(() => {
    void fetchData();
    pollRef.current = setInterval(() => void fetchData(), 5000);
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [fetchData]);

  const handleStartStream = async (channelId: number) => {
    setLoadingChannels(prev => new Set(prev).add(channelId));
    const res = await startStream(channelId);
    if (res.success && res.data) {
      setStreams(prev => { const next = new Map(prev); next.set(channelId, res.data!); return next; });
    }
    setTimeout(() => {
      setLoadingChannels(prev => { const next = new Set(prev); next.delete(channelId); return next; });
    }, 2000);
  };

  useEffect(() => {
    if (connectionStatus !== 'online' || devices.length === 0) return;
    const toStart = devices.filter(d => {
      const s = streams.get(d.channelId);
      return !(s?.status === 'running' || s?.status === 'starting') && !loadingChannels.has(d.channelId);
    });
    if (toStart.length === 0) return;
    const timers = toStart.map((d, i) => setTimeout(() => void handleStartStream(d.channelId), i * 1200));
    return () => timers.forEach(clearTimeout);
  }, [connectionStatus, devices, streams, loadingChannels]);

  return (
    <CameraGrid
      devices={devices}
      streams={streams}
      loadingChannels={loadingChannels}
    />
  );
}
