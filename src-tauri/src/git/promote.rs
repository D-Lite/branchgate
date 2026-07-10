use std::path::Path;
use std::process::Command;

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

pub fn delete_branch(path: &Path, branch: &str) -> Result<(), String> {
    let output = git(path, &["branch", "-D", branch])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn conflict_files(path: &Path) -> Result<Vec<String>, String> {
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

fn git(path: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| "Invalid path encoding".to_string())?;

    Command::new("git")
        .arg("-C")
        .arg(path_str)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))
}
