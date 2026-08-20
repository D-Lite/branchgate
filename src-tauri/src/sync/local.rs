use std::path::Path;

use crate::git::{self, DiffStats, GitCommit};

#[derive(Debug, Clone)]
pub struct LogicalUnit {
    pub merge_commit_sha: String,
    pub title: String,
    pub author: String,
    pub merged_at: i64,
    pub merge_strategy: &'static str,
    pub commit_shas: Vec<String>,
    pub patch_ids: Vec<String>,
    pub _diff: DiffStats,
}

pub fn units_from_commits(commits: Vec<GitCommit>) -> Vec<LogicalUnit> {
    commits.into_iter().map(unit_from_commit).collect()
}

pub fn unit_from_commit(commit: GitCommit) -> LogicalUnit {
    let strategy = if commit.parent_count > 1 {
        "merge"
    } else {
        "squash"
    };
    LogicalUnit {
        merge_commit_sha: commit.sha.clone(),
        title: commit.subject,
        author: commit.author,
        merged_at: commit.authored_at,
        merge_strategy: strategy,
        commit_shas: vec![commit.sha],
        patch_ids: Vec::new(),
        _diff: DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            changed_files: Vec::new(),
        },
    }
}

pub fn enrich_unit(path: &Path, unit: &mut LogicalUnit, target_head: &str) -> Result<(), String> {
    let patch = git::patch_id(path, &unit.merge_commit_sha)?;
    let diff = git::diff_vs_target(path, target_head, &unit.merge_commit_sha)?;
    unit.patch_ids = vec![patch];
    unit._diff = diff;
    Ok(())
}

pub fn parse_ticket_ref(title: &str) -> Option<String> {
    // Jira-style ABC-123 or Linear-style team-123
    let words: Vec<&str> = title.split_whitespace().collect();
    for word in words {
        if word.len() < 3 {
            continue;
        }
        if let Some(idx) = word.find('-') {
            if idx > 0 && word[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
                return Some(word.to_string());
            }
        }
    }
    None
}
