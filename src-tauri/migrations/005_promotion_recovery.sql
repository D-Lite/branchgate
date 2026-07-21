ALTER TABLE promotion_runs ADD COLUMN original_branch TEXT;

ALTER TABLE repos ADD COLUMN git_backend TEXT NOT NULL DEFAULT 'auto'
    CHECK (git_backend IN ('auto', 'native', 'wsl'));
ALTER TABLE repos ADD COLUMN wsl_distro TEXT;
