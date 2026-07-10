-- Branchgate — local SQLite schema (v1)
-- Portable to Postgres later via sqlx / sea-orm.

PRAGMA foreign_keys = ON;

CREATE TABLE repos (
    id                  INTEGER PRIMARY KEY,
    kind                TEXT NOT NULL CHECK (kind IN ('remote', 'local')),
    owner               TEXT,
    name                TEXT,
    remote_url          TEXT,
    local_path          TEXT,
    managed_clone_path  TEXT,
    working_copy_mode   TEXT NOT NULL DEFAULT 'managed'
                        CHECK (working_copy_mode IN ('managed', 'existing_local')),
    default_branch      TEXT,
    last_full_sync_at   INTEGER,
    created_at          INTEGER NOT NULL,
    UNIQUE (owner, name)
);

CREATE TABLE pipelines (
    id              INTEGER PRIMARY KEY,
    repo_id         INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    source_branch   TEXT NOT NULL,
    target_branch   TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    UNIQUE (repo_id, source_branch, target_branch)
);

CREATE TABLE branch_sync_state (
    pipeline_id         INTEGER PRIMARY KEY REFERENCES pipelines(id) ON DELETE CASCADE,
    source_head_sha     TEXT,
    target_head_sha     TEXT,
    last_synced_at      INTEGER
);

CREATE TABLE pull_requests (
    id                  INTEGER PRIMARY KEY,
    repo_id             INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    number              INTEGER,
    title               TEXT NOT NULL,
    author              TEXT,
    base_branch         TEXT NOT NULL,
    merge_strategy      TEXT CHECK (merge_strategy IN ('merge', 'squash', 'rebase', 'unknown')),
    merge_commit_sha    TEXT NOT NULL,
    ticket_ref          TEXT,
    url                 TEXT,
    merged_at           INTEGER,
    UNIQUE (repo_id, number)
);

CREATE TABLE pr_commits (
    id              INTEGER PRIMARY KEY,
    pr_id           INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    commit_sha      TEXT NOT NULL,
    patch_id        TEXT NOT NULL,
    authored_at     INTEGER,
    UNIQUE (pr_id, commit_sha)
);
CREATE INDEX idx_pr_commits_patch_id ON pr_commits(patch_id);

CREATE TABLE promotions (
    id                  INTEGER PRIMARY KEY,
    pipeline_id         INTEGER NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    pr_id               INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    status              TEXT NOT NULL CHECK (
                            status IN ('pending', 'selected', 'promoting', 'promoted', 'conflict', 'skipped')
                        ) DEFAULT 'pending',
    promoted_commit_sha TEXT,
    promoted_at         INTEGER,
    error_message       TEXT,
    UNIQUE (pipeline_id, pr_id)
);

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

CREATE TABLE promotion_run_prs (
    run_id      INTEGER NOT NULL REFERENCES promotion_runs(id) ON DELETE CASCADE,
    pr_id       INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    PRIMARY KEY (run_id, pr_id)
);

CREATE TABLE conflicts (
    id              INTEGER PRIMARY KEY,
    run_id          INTEGER NOT NULL REFERENCES promotion_runs(id) ON DELETE CASCADE,
    pr_id           INTEGER NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
    file_path       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'abandoned')) DEFAULT 'open',
    opened_in       TEXT,
    resolved_at     INTEGER
);

CREATE TABLE editors (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    command         TEXT NOT NULL,
    detected_path   TEXT,
    is_preferred    INTEGER NOT NULL DEFAULT 0,
    last_verified_at INTEGER
);

CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);
