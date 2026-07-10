# Branchgate

Selectively promote merged pull requests between branches — without merging entire branches, and without losing track of what's already moved.

**Platforms:** macOS · Windows (v1)

## Quick start

```bash
npm install
npm run tauri:dev
```

## Build

### Local

```bash
npm run tauri:build
```

Produces `.app` / `.dmg` on macOS and `.exe` / NSIS installer on Windows (build each on its own OS).

### GitHub Actions (private repo OK)

Pushing a version tag builds macOS (Apple Silicon + Intel) and Windows installers and opens a **draft** GitHub Release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Or run **Actions → Release → Run workflow** manually.

**One-time setup**

1. Repo **Settings → Actions → General → Workflow permissions** → *Read and write*
2. Repo **Settings → Secrets and variables → Actions** → add:
   - `VITE_POSTHOG_KEY` (required for analytics in production builds)
   - `VITE_POSTHOG_HOST` (optional; defaults to `https://us.i.posthog.com` in the app)

Installers appear under the draft release once all three matrix jobs finish (~15–25 min billed on the free tier).


## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system design.

## Product spec

The v1 proof-of-work and schema live in `docs/v1/`.
