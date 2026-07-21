use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    found.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });

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
    (
        "VS Code",
        "Visual Studio Code",
        "open -a 'Visual Studio Code'",
    ),
    ("Cursor", "Cursor", "open -a Cursor"),
    ("Zed", "Zed", "open -a Zed"),
];

fn resolve_cli(command: &str) -> Option<String> {
    #[cfg(windows)]
    let mut lookup = {
        let mut command_lookup = Command::new("where");
        command_lookup.arg(command);
        suppress_console_window(&mut command_lookup);
        command_lookup
    };
    #[cfg(not(windows))]
    let mut lookup = {
        let mut command_lookup = Command::new("which");
        command_lookup.arg(command);
        command_lookup
    };

    let output = lookup.output().ok()?;

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

pub fn open_file_with_path(
    editor_command: &str,
    detected_path: Option<&str>,
    file_path: &Path,
) -> Result<(), String> {
    if !file_path.exists() {
        return Err(format!("File not found: {}", file_path.display()));
    }

    let (mut command, is_gui) = if let Some(app_name) = macos_app_name(editor_command) {
        let mut command = Command::new("open");
        command.arg("-a").arg(app_name).arg("--").arg(file_path);
        (command, true)
    } else {
        let executable = detected_path
            .filter(|path| {
                let lower = path.to_ascii_lowercase();
                !lower.ends_with(".cmd") && !lower.ends_with(".bat")
            })
            .map(str::to_string)
            .or_else(|| resolve_cli(editor_command))
            .unwrap_or_else(|| editor_command.to_string());
        let mut command = Command::new(executable);
        command.arg(file_path);
        let is_gui = is_gui_editor(editor_command);
        if is_gui {
            suppress_console_window(&mut command);
        }
        (command, is_gui)
    };

    if is_gui {
        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to launch editor: {e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    } else {
        let status = command
            .status()
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
}

pub fn open_repo_with_path(
    editor_command: &str,
    detected_path: Option<&str>,
    repo_path: &Path,
) -> Result<(), String> {
    open_file_with_path(editor_command, detected_path, repo_path)
}

fn macos_app_name(editor_command: &str) -> Option<&str> {
    let app_name = editor_command.strip_prefix("open -a ")?.trim();
    let app_name = app_name
        .strip_prefix('\'')
        .and_then(|name| name.strip_suffix('\''))
        .unwrap_or(app_name);

    (!app_name.is_empty()).then_some(app_name)
}

fn is_gui_editor(editor_command: &str) -> bool {
    matches!(editor_command, "code" | "cursor" | "zed" | "subl")
}

fn suppress_console_window(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    #[cfg(not(windows))]
    let _ = command;
}
