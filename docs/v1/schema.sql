-- PR Resolver — local SQLite schema (v1)
-- Designed to be portable to Postgres later via the same query layer (sqlx / sea-orm).
-- All timestamps are UTC unix epoch integers.

PRAGMA foreign_keys = ON;

-- A repository the app knows about, either a remote GitHub repo (accessed via API)
-- or a local-only repo the user pointed at directly (no GitHub API access needed,
-- pure local git operations against the working copy at local_path).
CREATE TABLE repos (
    id                  INTEGER PRIMARY KEY,
    kind                TEXT NOT NULL CHECK (kind IN ('remote', 'local')),
    owner               TEXT,               -- GitHub org/user, null for local-only repos
    name                TEXT,               -- GitHub repo name, null for local-only repos
    remote_url          TEXT,               -- clone URL, null for local-only repos
    local_path          TEXT,               -- user's existing working copy, if working_copy_mode = 'existing_local'
    managed_clone_path  TEXT,               -- app-owned clone location, if working_copy_mode = 'managed'
    working_copy_mode   TEXT NOT NULL DEFAULT 'managed'
                        CHECK (working_copy_mode IN ('managed', 'existing_local')),
    default_branch      TEXT,
    last_full_sync_at   INTEGER,
    created_at          INTEGER NOT NULL,
    UNIQUE (owner, name)
);

-- A promotion pipeline: any (repo, source_branch, target_branch) triple the user has configured.
-- A repo can have many pipelines (develop->staging, staging->prod, feature/x->main, etc).
CREATE TABLE pipelines (
    id              INTEGER PRIMARY KEY,
    repo_id         INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,          -- user-facing label, e.g. "Develop to Staging"
    source_branch   TEXT NOT NULL,
    target_branch   TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    UNIQUE (repo_id, source_branch, target_branch)
);

-- Incremental sync checkpoint per pipeline, so re-syncs only process new commits.
CREATE TABLE branch_sync_state (
    pipeline_id         INTEGER PRIMARY KEY REFERENCES pipelines(id) ON DELETE CASCADE,
    source_head_sha     TEXT,
    target_head_sha     TEXT,
    last_synced_at      INTEGER
);

-- Pull requests discovered on a repo (scoped to repo, not pipeline — the same PR
-- can be relevant to multiple pipelines if branch structures overlap).
CREATE TABLE pull_requests (
    id                  INTEGER PRIMARY KEY,
    repo_id             INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    number              INTEGER,            -- null for local-only repos with no PR concept
    title               TEXT NOT NULL,
    author              TEXT,
    base_branch         TEXT NOT NULL,
    merge_strategy      TEXT CHECK (merge_strategy IN ('merge', 'squash', 'rebase', 'unknown')),
    merge_commit_sha    TEXT NOT NULL,
    ticket_ref          TEXT,               -- parsed Jira/Linear ref if present in title/body
    url                 TEXT,
    merged_at           INTEGER,
    UNIQUE (repo_id, number)
);

-- Commits belonging to a PR, with a content-based patch_id (git patch-id) so we can
-- detect "already applied" even after a cherry-pick changes the commit SHA.
CREATE TABLE pr_commits (
    id              INTEGER PRIMARY KEY,
    pr_id           INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    commit_sha      TEXT NOT NULL,
    patch_id        TEXT NOT NULL,
    authored_at     INTEGER,
    UNIQUE (pr_id, commit_sha)
);
CREATE INDEX idx_pr_commits_patch_id ON pr_commits(patch_id);

-- Per-pipeline promotion status of a PR: has it been checked, selected, promoted, conflicted, skipped.
CREATE TABLE promotions (
    id                  INTEGER PRIMARY KEY,
    pipeline_id         INTEGER NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    pr_id               INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    status              TEXT NOT NULL CHECK (
                            status IN ('pending', 'selected', 'promoting', 'promoted', 'conflict', 'skipped')
                        ) DEFAULT 'pending',
    promoted_commit_sha TEXT,               -- new SHA after cherry-pick onto target
    promoted_at         INTEGER,
    error_message       TEXT,
    UNIQUE (pipeline_id, pr_id)
);

-- A single "promote selected PRs" action: produces one branch + one PR to the target branch.
CREATE TABLE promotion_runs (
    id                  INTEGER PRIMARY KEY,
    pipeline_id         INTEGER NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    branch_name         TEXT NOT NULL,
    target_pr_number    INTEGER,
    target_pr_url       TEXT,
    status              TEXT NOT NULL CHECK (status IN ('running', 'open', 'merged', 'closed', 'failed')),
    created_at          INTEGER NOT NULL,
    completed_at        INTEGER
);

-- Which PRs went into a given promotion run (many-to-many).
CREATE TABLE promotion_run_prs (
    run_id      INTEGER NOT NULL REFERENCES promotion_runs(id) ON DELETE CASCADE,
    pr_id       INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    PRIMARY KEY (run_id, pr_id)
);

-- A merge conflict hit during a promotion run's cherry-pick sequence.
CREATE TABLE conflicts (
    id              INTEGER PRIMARY KEY,
    run_id          INTEGER NOT NULL REFERENCES promotion_runs(id) ON DELETE CASCADE,
    pr_id           INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    file_path       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'abandoned')) DEFAULT 'open',
    opened_in       TEXT,               -- editor id used to open it, if any
    resolved_at     INTEGER
);

-- Detected/registered code editors available for opening conflicts. Re-queryable
-- so the user can hit "detect editors" again after installing a new one.
CREATE TABLE editors (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,      -- "VS Code", "Cursor", "Zed"
    command         TEXT NOT NULL,             -- "code", "cursor", "zed"
    detected_path   TEXT,                      -- resolved binary path, null if only guessed
    is_preferred    INTEGER NOT NULL DEFAULT 0,
    last_verified_at INTEGER
);

-- App-level key/value settings: theme ('light'/'dark'/'system'), sync mode
-- ('manual'/'interval') and interval seconds, notification toggles, default
-- merge strategy assumption, etc. Secrets (GitHub tokens) live in the OS
-- keychain, never here — this table only stores non-secret config and
-- keychain reference keys.
CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);