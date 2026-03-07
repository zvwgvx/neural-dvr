const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3001';
const WS_BASE = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:3001';

export interface DvrInfo {
  deviceName: string;
  deviceId: string;
  model: string;
  serialNumber: string;
  firmwareVersion: string;
  encoderVersion: string;
  deviceType: string;
  channelCount: number;
}

export interface Channel {
  id: number;
  name: string;
  enabled: boolean;
  status: 'online' | 'offline' | 'unknown';
  resolutionWidth?: number;
  resolutionHeight?: number;
  videoCodec?: string;
}

export interface DiscoveredDevice {
  id: string;
  channelId: number;
  name: string;
  status: 'online' | 'offline' | 'unknown';
  ipAddress?: string;
  protocol?: string;
  resolution?: string;
  discoveredAt: string;
  lastSeen: string;
}

export interface StreamInfo {
  channelId: number;
  channelName: string;
  status: 'starting' | 'running' | 'stopped' | 'error';
  wsUrl: string;
  startedAt?: string;
  pid?: number;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

async function fetchApi<T>(path: string, options?: RequestInit): Promise<ApiResponse<T>> {
  try {
    const res = await fetch(`${API_BASE}${path}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    });
    return await res.json();
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Network error',
    };
  }
}

export async function getDvrInfo(): Promise<ApiResponse<DvrInfo>> {
  return fetchApi<DvrInfo>('/api/dvr/info');
}

export async function getDevices(): Promise<ApiResponse<DiscoveredDevice[]>> {
  return fetchApi<DiscoveredDevice[]>('/api/devices');
}

export async function getChannels(): Promise<ApiResponse<Channel[]>> {
  return fetchApi<Channel[]>('/api/channels');
}

export async function getStreams(): Promise<ApiResponse<StreamInfo[]>> {
  return fetchApi<StreamInfo[]>('/api/streams');
}

export async function startStream(channelId: number): Promise<ApiResponse<StreamInfo>> {
  return fetchApi<StreamInfo>(`/api/streams/${channelId}/start`, { method: 'POST' });
}

export async function stopStream(channelId: number): Promise<ApiResponse<string>> {
  return fetchApi<string>(`/api/streams/${channelId}/stop`, { method: 'POST' });
}

export function getWsStreamUrl(channelId: number): string {
  return `${WS_BASE}/ws/stream/${channelId}`;
}

export async function healthCheck(): Promise<boolean> {
  try {
    const res = await fetchApi<string>('/api/health');
    return res.success;
  } catch {
    return false;
  }
}
