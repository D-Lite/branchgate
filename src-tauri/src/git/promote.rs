use std::path::Path;

use super::runner::GitRunner;

pub struct CherryPickOutcome {
    pub success: bool,
    pub conflict_files: Vec<String>,
    pub stderr: String,
}

pub struct MergeOutcome {
    pub success: bool,
    pub conflict_files: Vec<String>,
    pub stderr: String,
}

pub fn current_branch(path: &Path) -> Result<String, String> {
    let output = git(path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !output.status.success() {
        return Err("Failed to read current branch".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn head_sha(path: &Path) -> Result<String, String> {
    let output = git(path, &["rev-parse", "HEAD"])?;
    if !output.status.success() {
        return Err("Failed to read HEAD".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn commit_exists(path: &Path, sha: &str) -> Result<bool, String> {
    let output = git(path, &["cat-file", "-e", &format!("{sha}^{{commit}}")])?;
    Ok(output.status.success())
}

pub fn ensure_clean(path: &Path) -> Result<(), String> {
    if operation_in_progress(path, "CHERRY_PICK_HEAD")?
        || operation_in_progress(path, "MERGE_HEAD")?
    {
        return Err(
            "Finish or abort the existing Git operation before starting a promotion".into(),
        );
    }
    let output = git(path, &["status", "--porcelain"])?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    if !output.stdout.is_empty() {
        return Err(
            "Repository has uncommitted changes. Commit or stash them before starting a promotion."
                .into(),
        );
    }
    Ok(())
}

pub fn switch_branch(path: &Path, branch: &str) -> Result<(), String> {
    let output = git(path, &["switch", branch])?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "Failed to switch to {branch}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub fn create_branch(path: &Path, branch: &str, base: &str) -> Result<(), String> {
    let output = git(path, &["switch", "-c", branch, base])?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "Failed to create branch {branch}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub fn cherry_pick(path: &Path, sha: &str, merge_commit: bool) -> CherryPickOutcome {
    let output = if merge_commit {
        git(path, &["cherry-pick", "-m", "1", sha])
    } else {
        git(path, &["cherry-pick", sha])
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return CherryPickOutcome {
                success: false,
                conflict_files: Vec::new(),
                stderr: e,
            };
        }
    };

    if output.status.success() {
        return CherryPickOutcome {
            success: true,
            conflict_files: Vec::new(),
            stderr: String::new(),
        };
    }

    let conflict_files = conflict_files(path).unwrap_or_default();
    CherryPickOutcome {
        success: false,
        conflict_files,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

pub fn cherry_pick_abort(path: &Path) -> Result<(), String> {
    let output = git(path, &["cherry-pick", "--abort"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn cherry_pick_continue(path: &Path) -> Result<(), String> {
    continue_operation(path, &["cherry-pick", "--continue"], "cherry-pick")
}

pub fn merge_branch(path: &Path, branch: &str) -> MergeOutcome {
    let output = match git(path, &["merge", "--no-edit", branch]) {
        Ok(o) => o,
        Err(e) => {
            return MergeOutcome {
                success: false,
                conflict_files: Vec::new(),
                stderr: e,
            };
        }
    };

    if output.status.success() {
        return MergeOutcome {
            success: true,
            conflict_files: Vec::new(),
            stderr: String::new(),
        };
    }

    let conflict_files = conflict_files(path).unwrap_or_default();
    MergeOutcome {
        success: false,
        conflict_files,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

pub fn merge_abort(path: &Path) -> Result<(), String> {
    let output = git(path, &["merge", "--abort"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn merge_continue(path: &Path) -> Result<(), String> {
    continue_operation(path, &["merge", "--continue"], "merge")
}

pub fn delete_branch(path: &Path, branch: &str) -> Result<(), String> {
    let output = git(path, &["branch", "-D", branch])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn conflict_files(path: &Path) -> Result<Vec<String>, String> {
    let output = git(path, &["diff", "--name-only", "--diff-filter=U"])?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn cherry_pick_in_progress(path: &Path) -> Result<bool, String> {
    operation_in_progress(path, "CHERRY_PICK_HEAD")
}

pub fn merge_in_progress(path: &Path) -> Result<bool, String> {
    operation_in_progress(path, "MERGE_HEAD")
}

pub fn is_ancestor(path: &Path, ancestor: &str, descendant: &str) -> Result<bool, String> {
    let output = git(path, &["merge-base", "--is-ancestor", ancestor, descendant])?;
    Ok(output.status.success())
}

fn operation_in_progress(path: &Path, revision: &str) -> Result<bool, String> {
    let output = git(path, &["rev-parse", "-q", "--verify", revision])?;
    Ok(output.status.success())
}

fn continue_operation(path: &Path, args: &[&str], label: &str) -> Result<(), String> {
    let unresolved = conflict_files(path)?;
    if !unresolved.is_empty() {
        return Err(format!(
            "{} conflicted file{} remain. Resolve and stage them before continuing.",
            unresolved.len(),
            if unresolved.len() == 1 { "" } else { "s" }
        ));
    }
    let output = git(path, args)?;
    if output.status.success() {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            format!("Failed to continue {label}")
        } else {
            error
        })
    }
}

fn git(path: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    GitRunner::for_repo(path).output(args)
}
