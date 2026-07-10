use std::path::Path;

use crate::git::{self, DiffStats};

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

pub fn discover_units(
    path: &Path,
    source: &str,
    target: &str,
    target_head: &str,
) -> Result<Vec<LogicalUnit>, String> {
    let commits = git::commits_ahead(path, source, target)?;
    let mut units = Vec::new();

    for commit in commits {
        let strategy = if commit.parent_count > 1 {
            "merge"
        } else {
            "squash"
        };
        let patch = git::patch_id(path, &commit.sha)?;
        let diff = git::diff_vs_target(path, target_head, &commit.sha)?;

        units.push(LogicalUnit {
            merge_commit_sha: commit.sha.clone(),
            title: commit.subject,
            author: commit.author,
            merged_at: commit.authored_at,
            merge_strategy: strategy,
            commit_shas: vec![commit.sha],
            patch_ids: vec![patch],
            _diff: diff,
        });
    }

    Ok(units)
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
