use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct GitCommit {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub authored_at: i64,
    pub parent_count: usize,
}

#[derive(Debug, Clone)]
pub struct DiffStats {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub changed_files: Vec<String>,
}

pub fn ensure_git_repo(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err("Path is not a directory".into());
    }
    if !path.join(".git").exists() {
        return Err("Not a git repository — pick a folder that contains a .git directory".into());
    }
    Ok(())
}

pub fn repo_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("local-repo")
        .to_string()
}

pub fn canonical_path(path: &Path) -> Result<String, String> {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("Failed to resolve path: {e}"))
}

pub fn current_branch(path: &Path) -> Result<Option<String>, String> {
    let output = git(path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch == "HEAD" {
        return Ok(None);
    }
    Ok(Some(branch))
}

pub fn list_branches(path: &Path) -> Result<Vec<String>, String> {
    let output = git(
        path,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to list branches: {stderr}"));
    }

    let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    branches.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    branches.dedup();
    Ok(branches)
}

pub fn branch_head(path: &Path, branch: &str) -> Result<String, String> {
    let output = git(path, &["rev-parse", &format!("refs/heads/{branch}")])?;
    if !output.status.success() {
        return Err(format!("Branch '{branch}' not found"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Commits on source's first-parent line that are not reachable from target.
pub fn commits_ahead(path: &Path, source: &str, target: &str) -> Result<Vec<GitCommit>, String> {
    let range = format!("{target}..{source}");
    let output = git(
        path,
        &[
            "log",
            "--first-parent",
            "--reverse",
            &format!("--format=%H\x1f%s\x1f%an\x1f%at"),
            &range,
        ],
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to read commit history: {stderr}"));
    }

    let mut commits = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 4 {
            continue;
        }
        let parent_count = parent_count(path, parts[0])?;
        commits.push(GitCommit {
            sha: parts[0].to_string(),
            subject: parts[1].to_string(),
            author: parts[2].to_string(),
            authored_at: parts[3].parse().unwrap_or(0),
            parent_count,
        });
    }
    Ok(commits)
}

pub fn parent_count(path: &Path, sha: &str) -> Result<usize, String> {
    let output = git(path, &["rev-list", "--parents", "-n", "1", sha])?;
    if !output.status.success() {
        return Ok(1);
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<_> = line.split_whitespace().collect();
    Ok(parts.len().saturating_sub(1).max(1))
}

fn is_merge_commit(path: &Path, sha: &str) -> bool {
    git(path, &["rev-parse", "--verify", &format!("{sha}^2")])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn patch_id(path: &Path, sha: &str) -> Result<String, String> {
    let is_merge = is_merge_commit(path, sha);

    // Plain `git show -p` on a merge commit produces no diff; use -m --first-parent.
    let show_output = if is_merge {
        git(
            path,
            &["show", "-m", "--first-parent", "-p", "--format=", sha],
        )?
    } else {
        git(path, &["show", "-p", "--format=", sha])?
    };

    if show_output.status.success() {
        if let Some(patch) = patch_id_from_bytes(&show_output.stdout) {
            return Ok(patch);
        }
    }

    let tree_output = if is_merge {
        git(path, &["diff-tree", "-p", "-m", "--first-parent", sha])?
    } else {
        git(path, &["diff-tree", "-p", &format!("{sha}^!")])?
    };
    if tree_output.status.success() {
        if let Some(patch) = patch_id_from_bytes(&tree_output.stdout) {
            return Ok(patch);
        }
    }

    // Empty commits / no-op merges — no diff to hash; track by SHA instead.
    Ok(format!("sha:{sha}"))
}

fn patch_id_from_bytes(diff: &[u8]) -> Option<String> {
    if diff.is_empty() {
        return None;
    }

    let mut child = Command::new("git")
        .args(["patch-id", "--stable"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        if stdin.write_all(diff).is_err() {
            return None;
        }
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&output.stdout);
    let patch = line.split_whitespace().next()?.to_string();
    if patch.is_empty() {
        None
    } else {
        Some(patch)
    }
}

pub fn cherry_on_target(path: &Path, upstream: &str, head: &str) -> Result<std::collections::HashMap<String, bool>, String> {
    let output = git(path, &["cherry", "-v", upstream, head])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git cherry failed: {stderr}"));
    }

    let mut map = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let marker = parts.next().unwrap_or("");
        let sha = parts.next().unwrap_or("");
        if sha.is_empty() {
            continue;
        }
        // "-" = patch already on upstream; "+" = not on upstream yet
        map.insert(sha.to_string(), marker == "-");
    }
    Ok(map)
}

pub fn diff_vs_target(
    path: &Path,
    target_sha: &str,
    commit_sha: &str,
) -> Result<DiffStats, String> {
    let output = git(
        path,
        &[
            "diff",
            "--numstat",
            &format!("{target_sha}..{commit_sha}"),
        ],
    )?;

    if !output.status.success() {
        return Ok(DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            changed_files: Vec::new(),
        });
    }

    let mut insertions = 0u32;
    let mut deletions = 0u32;
    let mut changed_files = Vec::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let add = parts.next().unwrap_or("-");
        let del = parts.next().unwrap_or("-");
        let file = parts.next().unwrap_or("").to_string();
        if add != "-" {
            insertions += add.parse().unwrap_or(0);
        }
        if del != "-" {
            deletions += del.parse().unwrap_or(0);
        }
        if !file.is_empty() {
            changed_files.push(file);
        }
    }

    Ok(DiffStats {
        files_changed: changed_files.len() as u32,
        insertions,
        deletions,
        changed_files,
    })
}

pub mod promote;

fn git(path: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| "Invalid path encoding".to_string())?;

    Command::new("git")
        .arg("-C")
        .arg(path_str)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git — is it installed and on PATH? {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn fixture_repo() -> std::path::PathBuf {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("test-branchgate/pr-resolver-fixture")
    }

    fn ensure_fixture() {
        let repo = fixture_repo();
        if repo.join(".git").exists() {
            return;
        }
        let script = fixture_repo()
            .parent()
            .unwrap()
            .join("generate-fixture-repo.sh");
        let status = Command::new("bash")
            .arg(&script)
            .arg(&repo)
            .status()
            .expect("run generate-fixture-repo.sh");
        assert!(status.success(), "fixture generator failed");
    }

    #[test]
    fn patch_id_for_every_commit_on_fixture_branches() {
        ensure_fixture();
        let repo = fixture_repo();
        for branch in ["develop", "staging", "qa", "uat", "master", "release"] {
            let output = git(&repo, &["rev-list", branch]).expect("rev-list");
            assert!(output.status.success());
            for sha in String::from_utf8_lossy(&output.stdout).lines() {
                let sha = sha.trim();
                if sha.is_empty() {
                    continue;
                }
                let patch = patch_id(&repo, sha).unwrap_or_else(|e| panic!("{e}"));
                assert!(
                    !patch.is_empty(),
                    "empty patch-id for {sha} on {branch}"
                );
            }
        }
    }

    #[test]
    fn patch_id_matches_for_promoted_auth_commit() {
        ensure_fixture();
        let repo = fixture_repo();
        let develop_merge = git(
            &repo,
            &[
                "log",
                "--first-parent",
                "--grep=Merge pull request #101",
                "-n",
                "1",
                "--format=%H",
                "develop",
            ],
        )
        .expect("git log");
        let develop_merge = String::from_utf8_lossy(&develop_merge.stdout)
            .trim()
            .to_string();
        let staging_head = git(&repo, &["rev-parse", "staging"])
            .expect("rev-parse")
            .stdout;
        let staging_head = String::from_utf8_lossy(&staging_head).trim().to_string();

        let develop_patch = patch_id(&repo, &develop_merge).expect("develop merge patch-id");
        let staging_patch = patch_id(&repo, &staging_head).expect("staging head patch-id");
        assert_eq!(
            develop_patch, staging_patch,
            "promoted auth should share patch-id between develop merge and staging"
        );
    }
}
