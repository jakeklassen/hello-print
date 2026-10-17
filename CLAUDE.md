# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Hello Print is a Tauri v2 desktop application that automates photo printing workflows. It watches a folder for .zip/image files, extracts images from zips, parses filenames for size keywords (e.g. `4x6`, `A4`), routes them to matching printer presets, and submits print jobs automatically. See `PRD.md` for full product requirements.

## Tech Stack

- **Frontend:** React 19, TypeScript, Vite 7, Tailwind CSS, shadcn/ui
- **Backend:** Rust (Tauri v2)
- **Package Manager:** pnpm
- **Required Toolchain:** Node 26.3.0, Rust 1.99.1, pnpm 11.5.3 (see `mise.toml`)
- **Platforms:** Windows (primary), macOS (secondary)

## Commands

```bash
pnpm install            # Install dependencies
pnpm dev                # Frontend dev server only (port 1420)
pnpm tauri dev          # Full Tauri app in dev mode (frontend + Rust backend)
pnpm build              # Type-check + bundle frontend
pnpm tauri build        # Build distributable app (.msi / .dmg)
```

No test framework is configured yet.

## Architecture

```
src/                  # React frontend
  main.tsx            # Entry point, renders App
  App.tsx             # Root component
src-tauri/            # Rust backend
  src/lib.rs          # Tauri commands (IPC handlers exposed to frontend)
  src/main.rs         # Binary entry, calls lib::run()
  tauri.conf.json     # App config: window size, dev URL, bundling, CSP
  capabilities/       # Tauri v2 permission declarations
  Cargo.toml          # Rust dependencies
```

**IPC pattern:** Frontend calls Rust functions via `@tauri-apps/api`. Rust commands are annotated with `#[tauri::command]` in `lib.rs` and registered in the Tauri builder.

**Vite config:** Dev server on port 1420, HMR on 1421. The `src-tauri/` directory is excluded from Vite's file watcher.

**Data storage:** Presets and app config stored as JSON in the Tauri app data directory.

### Platform Abstraction

Printing uses a unified Rust trait (`PrintService`) with platform-specific implementations behind conditional compilation (`#[cfg(target_os)]`):

- **Windows:** `winprint` crate for capability querying and Print Ticket-based submission, `printers` crate as fallback
- **macOS:** `cups_rs` for capability enumeration, CUPS API for job submission

### Key Rust Crates (planned)

`notify` (filesystem watching), `zip` (extraction), `printers` / `winprint` / `cups_rs` (printing), `image` (format validation), `serde` / `serde_json` (serialization)

### Two-Stage Pipeline

The core processing flow:

1. **Stage 1 — Zip extraction:** `.zip` detected in watch folder → extract images into watch folder → done
2. **Stage 2 — Image routing:** `.jpg/.png/.tiff` detected → parse filename for size keywords → match to preset → print

The image watcher is the single entry point for all printing; zip extraction just feeds into it.

### File Naming Convention

Filenames encode print instructions via size keywords (`4x6`, `5x7`, `8x10`, `8.5x11`/`letter`, `A4`-`A6`, plus aliases like `4R`, `KG`, `5R`). Keywords are case-insensitive, delimited by spaces/underscores/hyphens/dots. First keyword match wins; no match routes to the default preset.

### Data Models

- **AppConfig:** watch folder, tray behavior, default preset ID
- **Preset:** name, printer ID, paper size keyword, print settings, copies, auto-print flag, scale compensation
- **CustomKeyword:** keyword → paper dimensions → preset mapping (V2)
- **ProcessedFile:** filename + hash for dedup tracking

### UI Views

Dashboard (watch folder status, recent jobs), Presets Management (CRUD, calibration), Print Queue (jobs with thumbnails, cancel/retry/reprint), Settings (watch folder, tray, notifications)

## TypeScript Conventions

- Strict mode enabled
- No unused locals or parameters (`noUnusedLocals`, `noUnusedParameters`)
- Module resolution: bundler
- Target: ES2020
