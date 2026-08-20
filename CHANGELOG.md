# Changelog

## 0.1.2 — 2026-08-20

This release makes pipeline setup less brittle and keeps large repositories usable while they sync.

### Highlights

- Target branch selection can create a new local branch during pipeline setup, without switching your current checkout.
- Pipeline sync now loads commits in pages so the top of the list appears before the full history finishes scanning.
- The branch list on the connect flow stays current if you create a branch after opening BranchGate.

### Changed

- Syncing a pipeline streams checklist rows as each page lands, with progress in the header.
- Repeat syncs skip commits that are already indexed, so later refreshes stay closer to new activity.

### Fixed

- Fixed newly created branches missing from the source and target dropdowns until the connect flow was restarted.
- Fixed Check again enabling Continue promotion while the conflicted files and conflict state stayed on screen after they were resolved and staged.

### Upgrade notes

- Restart Branchgate after upgrading so the new Tauri commands are loaded.

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
