'use client';

import { useEffect, useRef, useState, useCallback } from 'react';

interface VideoPlayerProps {
    wsUrl: string;
    channelName: string;
    channelId: number;
    isStreaming: boolean;
    streamStatus?: string;
    loading?: boolean;
    hidden?: boolean;
    onDoubleClick?: () => void;
}

export default function VideoPlayer({
    wsUrl,
    channelName,
    channelId,
    isStreaming,
    streamStatus,
    loading,
    hidden,
    onDoubleClick,
}: VideoPlayerProps) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const [isPlaying, setIsPlaying] = useState(false);
    const [fps, setFps] = useState(0);
    const frameCountRef = useRef(0);

    // "Latest frame" decode pattern:
    // - pendingBlobRef holds the most recent frame received while a decode is in-flight
    // - isDecodingRef tracks whether a decode is currently happening
    // This ensures we never queue more than 1 pending frame and always draw the newest one.
    const isDecodingRef = useRef(false);
    const pendingBlobRef = useRef<Blob | null>(null);

    // FPS counter
    useEffect(() => {
        if (!isStreaming) return;
        const interval = setInterval(() => {
            setFps(frameCountRef.current);
            frameCountRef.current = 0;
        }, 1000);
        return () => clearInterval(interval);
    }, [isStreaming]);

    const drawBlobToCanvas = useCallback(async (blob: Blob) => {
        const canvas = canvasRef.current;
        if (!canvas) return;

        const tryDraw = async (b: Blob) => {
            try {
                if (typeof createImageBitmap === 'function') {
                    const bitmap = await createImageBitmap(b);
                    const ctx = canvas.getContext('2d');
                    if (ctx) {
                        if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
                            canvas.width = bitmap.width;
                            canvas.height = bitmap.height;
                        }
                        ctx.drawImage(bitmap, 0, 0);
                        bitmap.close();
                        frameCountRef.current += 1;
                        setIsPlaying(true);
                    }
                } else {
                    await new Promise<void>((resolve, reject) => {
                        const url = URL.createObjectURL(b);
                        const img = new Image();
                        img.onload = () => {
                            try {
                                const ctx = canvas.getContext('2d');
                                if (ctx) {
                                    if (canvas.width !== img.width || canvas.height !== img.height) {
                                        canvas.width = img.width;
                                        canvas.height = img.height;
                                    }
                                    ctx.drawImage(img, 0, 0);
                                    frameCountRef.current += 1;
                                    setIsPlaying(true);
                                }
                                resolve();
                            } catch (e) { reject(e); } finally { URL.revokeObjectURL(url); }
                        };
                        img.onerror = () => { URL.revokeObjectURL(url); reject(); };
                        img.src = url;
                    });
                }
            } catch {
                // Ignore malformed frames
            }
        };

        if (isDecodingRef.current) {
            // A decode is in progress — just save the latest blob and return.
            // The loop below will pick it up after the current decode finishes.
            pendingBlobRef.current = blob;
            return;
        }

        // Start decode loop: process incoming blob, and keep looping if new frames arrived
        isDecodingRef.current = true;
        let current: Blob = blob;
        while (true) {
            await tryDraw(current);
            // Check if a newer frame arrived while we were decoding
            const next = pendingBlobRef.current;
            if (next) {
                pendingBlobRef.current = null;
                current = next;
            } else {
                break;
            }
        }
        isDecodingRef.current = false;
    }, []);

    // WebSocket connection for MJPEG frames
    useEffect(() => {
        if (!isStreaming) {
            setIsPlaying(false);
            return;
        }

        const connectWs = () => {
            const ws = new WebSocket(wsUrl);
            ws.binaryType = 'arraybuffer';

            ws.onopen = () => setIsPlaying(false);

            ws.onmessage = (event) => {
                let blob: Blob;
                if (event.data instanceof ArrayBuffer) {
                    blob = new Blob([event.data], { type: 'image/jpeg' });
                } else if (event.data instanceof Blob) {
                    blob = event.data;
                } else return;
                void drawBlobToCanvas(blob);
            };

            ws.onerror = () => setIsPlaying(false);
            ws.onclose = () => {
                setIsPlaying(false);
                setTimeout(() => {
                    if (wsRef.current === ws) { wsRef.current = null; connectWs(); }
                }, 3000);
            };
            wsRef.current = ws;
        };

        const timer = setTimeout(connectWs, 2000);
        return () => {
            clearTimeout(timer);
            if (wsRef.current) { wsRef.current.close(); wsRef.current = null; }
            setIsPlaying(false);
            pendingBlobRef.current = null;
        };
    }, [isStreaming, wsUrl, drawBlobToCanvas]);

    if (hidden) return null;

    return (
        <div
            className={`video-card ${isStreaming ? 'streaming' : ''}`}
            onDoubleClick={onDoubleClick}
        >
            <div className="video-wrapper">
                {isStreaming ? (
                    <>
                        <canvas
                            ref={canvasRef}
                            id={`video-canvas-${channelId}`}
                            style={{
                                position: 'absolute',
                                top: 0,
                                left: 0,
                                width: '100%',
                                height: '100%',
                                objectFit: 'cover',
                            }}
                        />
                        <div className="video-overlay">
                            <div className="video-live-badge">
                                <span className="video-live-dot" />
                                LIVE {isPlaying && fps > 0 ? `· ${fps}fps` : ''}
                            </div>
                            <div className="video-channel-label">{channelName}</div>
                        </div>
                    </>
                ) : (
                    <div className="video-placeholder">
                        <div className="video-placeholder-text">
                            {loading ? 'Starting stream...' : 'Waiting for stream...'}
                        </div>
                        <div className="video-channel-label-idle">{channelName}</div>
                        {loading && <div className="spinner" style={{ width: 20, height: 20 }} />}
                    </div>
                )}
            </div>
        </div>
    );
}
