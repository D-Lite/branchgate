ALTER TABLE promotion_runs ADD COLUMN conflict_phase TEXT
    CHECK (conflict_phase IS NULL OR conflict_phase IN ('cherry_pick', 'merge'));
