#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${BACKEND_DIR}"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "error: ffmpeg is required but not installed." >&2
  echo "macOS: brew install ffmpeg" >&2
  echo "Ubuntu/Debian: sudo apt-get install -y ffmpeg" >&2
  echo "Fedora: sudo dnf install -y ffmpeg" >&2
  exit 1
fi

echo "Using $(ffmpeg -version | head -n 1)"
exec cargo run
