# Neural DVR — Hikvision DVR Realtime Streaming

Hệ thống kết nối đến đầu thu Hikvision DVR bằng RTSP channel tĩnh (`DVR_CHANNELS`) và stream video realtime lên website.

## Architecture

```
┌─────────────────┐     RTSP stream        ┌─────────────────┐
│  Hikvision DVR  │ ─────────────────────▶ │  Rust Backend   │
│  Camera 1..N    │                        │  (axum + ffmpeg)│
└─────────────────┘          │             └────────┬────────┘
                      ┌──────▼─────┐                │ REST API
                      │  ffmpeg    │                │ + WebSocket
                      │ image2pipe │── JPEG ──────▶ │   MJPEG
                      └────────────┘                │
                                           ┌────────┴────────┐
                                           │  Next.js        │
                                           │  Frontend       │
                                           │  (WS + Canvas)  │
                                           └─────────────────┘
```

## Requirements

- **Rust** >= 1.75 (stable)
- **Node.js** >= 18
- **ffmpeg** available in `PATH`
- **Hikvision DVR** accessible qua mạng LAN

Install ffmpeg by platform:

```bash
# macOS (Homebrew)
brew install ffmpeg

# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y ffmpeg

# Fedora
sudo dnf install -y ffmpeg
```

## Quick Start

### 1. Configure DVR connection

```bash
cp backend/.env.example backend/.env
# Edit backend/.env with your DVR info
```

```env
DVR_HOST=192.168.1.64
DVR_PORT=80
DVR_RTSP_PORT=554
DVR_USERNAME=admin
DVR_PASSWORD=your_password_here
# Required: direct RTSP channel IDs
DVR_CHANNELS=101,201,301,401,501,601
```

### 2. Run Backend (cross-platform helper)

```bash
make backend
# alias ngắn: make be
```

Server sẽ chạy tại `http://localhost:3001`

Nếu bạn chưa cài package frontend:

```bash
make frontend-install
```

### 3. Run Frontend

```bash
make frontend
# alias ngắn: make fe
```

Mở browser tại `http://localhost:3000`

Nếu bạn muốn chạy manual thay vì `make`:

```bash
cd backend

cargo run
```

## API Endpoints

| Route | Method | Description |
|-------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/dvr/info` | GET | DVR device info |
| `/api/devices` | GET | Cameras from `DVR_CHANNELS` |
| `/api/devices/:id` | GET | Camera detail |
| `/api/channels` | GET | Configured RTSP channels |
| `/api/streams` | GET | Active streams |
| `/api/streams/:ch/start` | POST | Start streaming |
| `/api/streams/:ch/stop` | POST | Stop streaming |
| `/ws/stream/:ch` | WS | WebSocket MJPEG stream |

## Project Structure

```
neural-dvr/
├── backend/                    # Rust backend
│   ├── Cargo.toml
│   ├── .env                    # DVR configuration
│   ├── scripts/
│   │   └── run-backend.sh      # macOS/Linux ffmpeg helper
│   └── src/
│       ├── main.rs             # Entry point
│       ├── config.rs           # .env loading
│       ├── models.rs           # Data structures
│       ├── api.rs              # REST API + WebSocket
│       ├── streaming.rs        # ffmpeg RTSP→MJPEG
├── frontend/                   # Next.js frontend
│   └── src/
│       ├── app/
│       │   ├── layout.tsx
│       │   ├── page.tsx        # Dashboard
│       │   └── globals.css     # Dark theme
│       ├── components/
│       │   ├── VideoPlayer.tsx  # WS MJPEG player
│       │   ├── DeviceList.tsx   # Camera sidebar
│       │   └── CameraGrid.tsx   # Grid layout
│       └── lib/
│           └── api.ts          # API client
└── README.md
```

## Features

- 🎯 **Direct Channel Mode** — Dùng trực tiếp `DVR_CHANNELS` (không phụ thuộc ISAPI)
- 📹 **FFmpeg Streaming** — RTSP capture qua ffmpeg, JPEG qua WebSocket
- 🎛️ **Dashboard** — Giao diện dark theme premium
- 📐 **Grid Layout** — Xem nhiều camera cùng lúc (1×1, 2×2, 3×3, 4×4)
- 🔄 **Auto-Reconnect** — WebSocket tự kết nối lại khi mất kết nối
- ⚡ **Low Latency** — Direct frame push, không qua HLS segments
- 📊 **FPS Counter** — Hiển thị FPS realtime trên mỗi camera

## License

See [LICENSE](./LICENSE)
