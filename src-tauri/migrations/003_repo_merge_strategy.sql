ALTER TABLE repos ADD COLUMN default_merge_strategy TEXT NOT NULL DEFAULT 'auto'
    CHECK (default_merge_strategy IN ('auto', 'merge', 'squash', 'rebase'));
