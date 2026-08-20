use std::path::Path;

use runner::GitRunner;

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
    let output = git(path, &["rev-parse", "--is-inside-work-tree"])?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "true" {
        return Err("Not a Git working tree — choose a repository folder or subfolder".into());
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

/// Create a local branch at `base` without switching the current checkout.
pub fn create_branch_ref(path: &Path, branch: &str, base: &str) -> Result<(), String> {
    let name = branch.trim();
    if name.is_empty() {
        return Err("Branch name is required".into());
    }

    let format = git(path, &["check-ref-format", "--branch", name])?;
    if !format.status.success() {
        return Err(format!("'{name}' is not a valid Git branch name"));
    }

    let normalized = String::from_utf8_lossy(&format.stdout)
        .trim()
        .to_string();
    let branch_name = if normalized.is_empty() {
        name.to_string()
    } else {
        normalized
    };

    let existing = list_branches(path)?;
    if existing.iter().any(|candidate| candidate == &branch_name) {
        return Err(format!("Branch '{branch_name}' already exists"));
    }
    if !existing.iter().any(|candidate| candidate == base) {
        return Err(format!("Base branch '{base}' not found"));
    }

    let output = git(path, &["branch", "--", &branch_name, base])?;
    if !output.status.success() {
        return Err(format!(
            "Failed to create branch {branch_name}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn branch_head(path: &Path, branch: &str) -> Result<String, String> {
    let output = git(path, &["rev-parse", &format!("refs/heads/{branch}")])?;
    if !output.status.success() {
        return Err(format!("Branch '{branch}' not found"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Commits on source's first-parent line that are not reachable from target.
/// Oldest commits come first so the checklist can fill from the top.
pub fn commits_ahead(path: &Path, source: &str, target: &str) -> Result<Vec<GitCommit>, String> {
    let range = format!("{target}..{source}");
    let output = git(
        path,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H\x1f%s\x1f%an\x1f%at\x1f%P",
            &range,
        ],
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to read commit history: {stderr}"));
    }

    Ok(parse_commit_lines(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_commit_lines(stdout: &str) -> Vec<GitCommit> {
    let mut commits = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 4 {
            continue;
        }
        let parent_count = parts
            .get(4)
            .map(|parents| parents.split_whitespace().count())
            .unwrap_or(0)
            .max(1);
        commits.push(GitCommit {
            sha: parts[0].to_string(),
            subject: parts[1].to_string(),
            author: parts[2].to_string(),
            authored_at: parts[3].parse().unwrap_or(0),
            parent_count,
        });
    }
    commits
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
        if let Some(patch) = patch_id_from_bytes(path, &show_output.stdout) {
            return Ok(patch);
        }
    }

    let tree_output = if is_merge {
        git(path, &["diff-tree", "-p", "-m", "--first-parent", sha])?
    } else {
        git(path, &["diff-tree", "-p", &format!("{sha}^!")])?
    };
    if tree_output.status.success() {
        if let Some(patch) = patch_id_from_bytes(path, &tree_output.stdout) {
            return Ok(patch);
        }
    }

    // Empty commits / no-op merges — no diff to hash; track by SHA instead.
    Ok(format!("sha:{sha}"))
}

fn patch_id_from_bytes(path: &Path, diff: &[u8]) -> Option<String> {
    if diff.is_empty() {
        return None;
    }

    let output = GitRunner::for_repo(path)
        .output_with_input(["patch-id", "--stable"], diff)
        .ok()?;
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
pub mod runner;

fn git(path: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    GitRunner::for_repo(path)
        .output(args)
        .map_err(|e| format!("{e} — install Git for Windows or enable WSL Git for this repository"))
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
    fn parse_commit_lines_reads_parent_hashes() {
        let stdout = "abc\x1fMerge PR\x1fAda\x1f1700000000\x1fparent1 parent2\n\
                      def\x1fSquash\x1fAda\x1f1700000001\x1fparent1\n";
        let commits = parse_commit_lines(stdout);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parent_count, 2);
        assert_eq!(commits[1].parent_count, 1);
        assert_eq!(commits[0].subject, "Merge PR");
    }

    #[test]
    fn create_branch_ref_leaves_current_checkout() {
        let repo = unique_temp_repo();
        init_temp_repo(&repo);
        let before = current_branch(&repo).unwrap();
        create_branch_ref(&repo, "release/next", "main").expect("create branch");
        let after = current_branch(&repo).unwrap();
        assert_eq!(before, after);
        let branches = list_branches(&repo).unwrap();
        assert!(branches.iter().any(|branch| branch == "release/next"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    fn unique_temp_repo() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "create-branch-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
    }

    fn init_temp_repo(repo: &std::path::Path) {
        std::fs::create_dir_all(repo).unwrap();
        assert!(git(repo, &["init", "-b", "main"]).unwrap().status.success());
        git(repo, &["config", "user.email", "test@example.com"]).unwrap();
        git(repo, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(repo.join("README.md"), "ok").unwrap();
        assert!(git(repo, &["add", "README.md"]).unwrap().status.success());
        assert!(
            git(repo, &["commit", "-m", "init"])
                .unwrap()
                .status
                .success()
        );
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
