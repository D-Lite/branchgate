# Changelog

## 0.1.1 — 2026-07-21

This release makes promotion recovery substantially safer and improves Branchgate's Windows desktop experience.

### Highlights

- Added native Windows Git and WSL Git support with automatic repository-path detection and per-repository overrides.
- Reworked conflicted promotions into a recoverable workflow with live Git status checks, automatic conflict reconciliation, and a clear Continue promotion action.
- Distinguished genuine merge conflicts from Git failures such as missing or stale commit objects.
- Added pipeline deletion with confirmation, preserved promotion history, and a Deselect all action.
- Added an offline analytics queue that retries delivery when connectivity returns.

### Changed

- Git commands now run through a shared platform-aware runner and suppress unwanted console windows on Windows.
- Conflict dialogs now refresh while open, respond when the app regains focus, and provide clearer resolution instructions.
- Promotion runs now remember their original branch and can recover after Branchgate restarts.
- Editor detection and preference updates retain stable alphabetical ordering.
- Pipeline refreshes now retire stale pending commits after source history changes.
- Conflict and deletion dialogs have improved responsive layouts, keyboard navigation, focus management, and narrow-screen behavior.
- GUI editors such as Zed launch without blocking Branchgate.

### Fixed

- Fixed terminal windows flickering during Git operations on Windows.
- Fixed resolved conflicts remaining visible after files were staged.
- Fixed failed Git commands being presented as editable conflicts.
- Fixed stale commit IDs causing repeated `fatal: bad object` promotion failures.
- Fixed clipped modal actions, horizontal scrolling, and poorly positioned buttons.
- Fixed successful promotion dialogs incorrectly retaining an Abort action.
- Fixed preferred editors jumping position when selected.

### Upgrade notes

- The local database is migrated automatically on startup.
- Restart Branchgate after upgrading so the new Tauri commands and migrations are loaded.
