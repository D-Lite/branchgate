# Branchgate

Selectively promote merged pull requests between branches — without merging entire branches, and without losing track of what's already moved.

**Platforms:** macOS · Windows (v1)

## Quick start

```bash
npm install
npm run tauri:dev
```

## Build

```bash
npm run tauri:build
```

Produces `.app` / `.dmg` on macOS and `.exe` / NSIS installer on Windows.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system design.

## Product spec

The v1 proof-of-work and schema live in `docs/v1/`.
