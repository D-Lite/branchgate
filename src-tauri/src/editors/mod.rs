use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct DetectedEditor {
    pub name: String,
    pub command: String,
    pub detected_path: Option<String>,
}

pub fn detect() -> Vec<DetectedEditor> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (name, command) in CLI_EDITORS {
        if seen.contains(name) {
            continue;
        }
        if let Some(path) = resolve_cli(command) {
            seen.insert(name);
            found.push(DetectedEditor {
                name: name.to_string(),
                command: command.to_string(),
                detected_path: Some(path),
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        for (name, app_name, command) in MAC_APP_EDITORS {
            if seen.contains(name) {
                continue;
            }
            let app_path = format!("/Applications/{app_name}.app");
            if Path::new(&app_path).exists() {
                seen.insert(name);
                found.push(DetectedEditor {
                    name: name.to_string(),
                    command: command.to_string(),
                    detected_path: Some(app_path),
                });
            }
        }
    }

    found
}

const CLI_EDITORS: &[(&str, &str)] = &[
    ("VS Code", "code"),
    ("Cursor", "cursor"),
    ("Zed", "zed"),
    ("Sublime Text", "subl"),
    ("Neovim", "nvim"),
    ("Vim", "vim"),
];

#[cfg(target_os = "macos")]
const MAC_APP_EDITORS: &[(&str, &str, &str)] = &[
    ("VS Code", "Visual Studio Code", "open -a 'Visual Studio Code'"),
    ("Cursor", "Cursor", "open -a Cursor"),
    ("Zed", "Zed", "open -a Zed"),
];

fn resolve_cli(command: &str) -> Option<String> {
    #[cfg(windows)]
    let output = Command::new("where").arg(command).output().ok()?;
    #[cfg(not(windows))]
    let output = Command::new("which").arg(command).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();

    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

pub fn open_file(editor_command: &str, file_path: &Path) -> Result<(), String> {
    if !file_path.exists() {
        return Err(format!("File not found: {}", file_path.display()));
    }

    let path_str = file_path
        .to_str()
        .ok_or_else(|| "Invalid file path encoding".to_string())?;

    let status = if editor_command.starts_with("open ") {
        Command::new("sh")
            .arg("-lc")
            .arg(format!("{editor_command} -- {path_str}"))
            .status()
    } else {
        Command::new(editor_command).arg(path_str).status()
    }
    .map_err(|e| format!("Failed to launch editor: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Editor exited with status {}",
            status.code().unwrap_or(-1)
        ))
    }
}

pub fn open_repo(editor_command: &str, repo_path: &Path) -> Result<(), String> {
    open_file(editor_command, repo_path)
}
