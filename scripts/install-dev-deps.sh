#!/usr/bin/env bash
# Install system dependencies required to build pelagos-ui on Linux.
# Run once after cloning the repo.
set -euo pipefail

if command -v pacman &>/dev/null; then
    sudo pacman -S --needed \
        webkit2gtk-4.1 \
        gtk3 \
        libayatana-appindicator \
        base-devel \
        curl \
        wget \
        openssl
elif command -v apt &>/dev/null; then
    sudo apt install -y \
        libwebkit2gtk-4.1-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        build-essential \
        curl \
        wget \
        libssl-dev
else
    echo "Unknown package manager — install webkit2gtk-4.1, gtk3, and libayatana-appindicator3 manually"
    exit 1
fi

echo "System deps installed. Run: npm install && cargo build -p pelagos-ui"
