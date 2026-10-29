<div align="center">

# ClipMaker

**A Medal-style replay recorder for Windows, written in Rust.**

[![Latest release](https://img.shields.io/github/v/release/drazoxXD/ClipMaker?style=for-the-badge&label=release)](https://github.com/drazoxXD/ClipMaker/releases/latest)
[![Build](https://img.shields.io/github/actions/workflow/status/drazoxXD/ClipMaker/build.yml?style=for-the-badge&label=build)](https://github.com/drazoxXD/ClipMaker/actions/workflows/build.yml)
![Platform](https://img.shields.io/badge/Windows-10%20%2F%2011-0078D6?style=for-the-badge)
![Stack](https://img.shields.io/badge/Tauri%202-Rust%20%2B%20React-orange?style=for-the-badge)

[Download](https://github.com/drazoxXD/ClipMaker/releases/latest/download/ClipMaker_Setup.exe) · [Getting started](#getting-started) · [Settings](#settings) · [How it works](#how-it-works) · [Building](#building)
</div>

> [!WARNING]
> ClipMaker is Work In Progress and under active development. \
> ClipMaker uses AI for development and in stable version will refactor the whole codebase.

---

## About

ClipMaker keeps the last few minutes of your screen in a replay buffer and saves them as a clip when you press a hotkey, like Medal or AMD ReLive's Instant Replay, and nothing more. It records the screen or the game's window, and never injects anything into a game.

Ultrawide is first class: a 3440×1440 monitor at the 1080p preset records 2580×1080, so 21:9 stays 21:9 with no black bars.

## Features

| | |
|---|---|
| **Replay buffer** | 15 seconds to 30 minutes, kept in memory or in rolling files in a cache folder, with a live size estimate in MB. Saving copies the already-encoded packets: no re-encode, about 0.1–0.4 s per clip. |
| **Game window capture** | Follows the fullscreen game in front, the way Game DVR does, and falls back to the whole monitor otherwise. Monitor capture (Graphics Capture or DXGI) is there too. No injection. |
| **GPU or CPU encoding** | AMD AMF, NVIDIA NVENC and Intel Quick Sync (H.264, HEVC, AV1), or x264, x265 and SVT-AV1. Only encoders that work on your PC are offered. AMF and NVENC run zero-copy: frames never leave the GPU. |
| **Every option** | Resolution presets or custom size, frame rate, CBR/VBR/CQP/CRF, bitrate, max bitrate, preset, profile, tune, keyframe interval, B-frames, threads, color range, MP4 or MKV. |
| **Per-app audio** | Every app gets its own channel with its own volume (0–200%) and mute, mixed with your mic into one stereo track. Apps can also get a track of their own for editing; apps that stayed silent are left out of the clip. |
| **Microphone** | Own track or mixed in, RNNoise noise suppression, a noise gate with a live "they can hear you" meter, and Listen to hear yourself exactly as recorded. |
| **Hotkeys that work in games** | Read through Raw Input like Medal, so they keep working in games that block normal hotkeys (Rainbow Six Siege, for one). |
| **Overlay** | "Saving" and "Saved" notifications on top of the game, invisible to the recording, and only on screen while a notification is up. |
| **Tray and autostart** | Lives in the tray, can start with Windows and start buffering on launch. |
| **English and Hungarian** | Every string lives in `src/locales/*.json`; adding a language is one file. |

## Getting started

1. Download [`ClipMaker_Setup.exe`](https://github.com/drazoxXD/ClipMaker/releases/latest/download/ClipMaker_Setup.exe) from the latest release and run it.
2. ClipMaker opens and starts the replay buffer. The Overview page shows what is being captured.
3. Press **F8** (change it on the Hotkeys page) to save the last minute. Clips go to `Videos\Drazox Recorder`.

Closing the window keeps ClipMaker running in the tray; quit from the tray menu.

## Settings

| Page | What you set |
|---|---|
| **Video** | Capture method (Game window, Monitor, DXGI), monitor, priority, resolution, frame rate, encoder and every rate-control option. |
| **Audio** | Mix track mode, per-app volume and own tracks, microphone, noise suppression and gate, audio codec and bitrate. |
| **Replay & Storage** | Replay length with the size estimate, memory or disk buffer and its cache folder, clips folder, container and file name template (`{app}_{date}_{time}`). |
| **Hotkeys** | Save clip, start or stop replay, show the window. |
| **General** | Language, start with Windows, start minimized, start replay on launch, save notification and sound. |

## How it works

Every stage runs on its own thread:

- **Capture:** Windows Graphics Capture (or DXGI duplication) hands over a D3D11 texture. A D3D11 video processor scales it and converts it to NV12 on the GPU.
- **Encode:** AMF or NVENC take that texture directly through FFmpeg's D3D11 hardware frames. CPU encoders get it read back through a staging ring.
- **Audio:** WASAPI process loopback captures each app on its own, and a mixer builds the mix track on a shared QPC clock. The mic goes through RNNoise, volume and the gate.
- **Buffer:** encoded packets, trimmed at keyframes, in memory or in 2-second segment files.
- **Save:** the packets since the right keyframe are remuxed into MP4 or MKV, with one named track per source.

## Building

You need [Rust](https://rustup.rs), [Node.js](https://nodejs.org) and [LLVM](https://llvm.org) (for the FFmpeg bindings; `winget install LLVM.LLVM`).

```bash
# FFmpeg 9.0 shared build into src-tauri/ffmpeg (headers, import libs and DLLs)
pwsh -File scripts/fetch-ffmpeg.ps1

npm install
npm run tauri dev      # development, with the UI hot-reloading
npm run tauri build    # the installer, in src-tauri/target/release/bundle/nsis
```

`scripts/release-install.ps1` builds the installer, closes the running app, installs the new version silently and starts it.

Releases are built by the [Build workflow](.github/workflows/build.yml) on every code change, or by hand. Each release lists the commits since the previous one and how long the build took.

---

<div align="center">

ClipMaker is not affiliated with or endorsed by Medal, AMD, NVIDIA or Microsoft. It bundles FFmpeg (GPL build by BtbN) and uses RNNoise through nnnoiseless.

</div>
