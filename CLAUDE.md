# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ALVR (Air Light VR) streams VR games from PC to standalone VR headsets over Wi-Fi. The codebase is a Rust workspace with some C++ components for graphics and SteamVR integration.

## Build Commands

All build tasks use the custom `cargo xtask` system:

```bash
# Prepare dependencies (required before first build)
cargo xtask prepare-deps --platform windows          # Windows with NVIDIA
cargo xtask prepare-deps --platform linux --no-nvidia # Linux AMD
cargo xtask prepare-deps --platform android          # Android client

# Build streamer (dashboard + SteamVR driver)
cargo xtask build-streamer --release
cargo xtask build-streamer --release --gpl  # Windows: bundle FFmpeg

# Build Android client
cargo xtask build-client --release

# Build launcher
cargo xtask build-launcher --release

# Run streamer (builds if needed, then launches dashboard)
cargo xtask run-streamer
cargo xtask run-streamer --no-rebuild  # Skip rebuild

# Run launcher
cargo xtask run-launcher
```

## Code Quality

```bash
# Format code (Rust + C++ via clang-format)
cargo xtask format

# Check formatting (CI uses this)
cargo xtask check-format

# Run clippy
cargo xtask clippy
cargo xtask clippy --ci  # CI mode

# Run tests
cargo test -p alvr_session
```

## Architecture

### Two-Application Model
- **Streamer** (Windows/Linux): Dashboard GUI + SteamVR driver for capturing/encoding VR frames
- **Client** (Android): OpenXR app on VR headset that receives/decodes/displays frames

### Key Crates (`alvr/`)
- `server_core/`: Driver logic - client discovery, streaming, encoding coordination
- `server_openvr/`: SteamVR driver interface (C++ in `cpp/` subdirectory)
- `client_core/`: Platform-agnostic client code, can build as C-ABI library
- `client_openxr/`: OpenXR-based Android client (builds to APK)
- `dashboard/`: Settings UI, client management, statistics
- `sockets/`: Network protocol for client-driver communication
- `session/`: Configuration management (`session.json`), settings schema
- `packets/`: Packet definitions for client-driver-dashboard communication
- `xtask/`: Build system and development scripts

### Communication
- Discovery: UDP broadcast on port 9943
- Control: TCP socket for reliable small messages
- Streaming: UDP or TCP for video/audio/tracking data
- Dashboard-Driver: HTTP API at `http://localhost:8082`

### Video Pipeline
1. SteamVR renders game frame
2. Driver captures frame (DirectX 11 on Windows, Vulkan layer on Linux)
3. Foveated encoding compresses image periphery
4. Hardware encode (NvEnc/AMF/VAAPI) to h264/HEVC
5. Network transmission with packet sharding
6. Client decodes via MediaCodec
7. Foveated decoding restores image
8. Submit to headset VR runtime

### Languages
- **Rust**: Main language - dashboard, networking, client core, audio
- **C++**: Graphics, video encoding, SteamVR integration (`alvr/server_openvr/cpp/`)
- **GLSL**: Shaders for Linux driver and client compositor
- **HLSL**: Shaders for Windows driver compositor

## Environment Variables (Android builds)

Required for building the Android client:
- `JAVA_HOME`: JDK path
- `ANDROID_HOME`: Android SDK path
- `ANDROID_NDK_HOME`: NDK path (currently v26b)

## Rust Version

The workspace requires Rust 1.88+ (see `rust-version` in root `Cargo.toml`).
