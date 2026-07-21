use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{OnceLock, RwLock};

static REPO_BACKENDS: OnceLock<RwLock<HashMap<String, (String, Option<String>)>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitBackend {
    Native,
    Wsl { distro: String, linux_path: String },
}

#[derive(Debug, Clone)]
pub struct GitRunner {
    backend: GitBackend,
    repo_path: PathBuf,
}

impl GitRunner {
    pub fn for_repo(path: &Path) -> Self {
        Self {
            backend: configured_backend(path).unwrap_or_else(|| detect_backend(path)),
            repo_path: path.to_path_buf(),
        }
    }

    pub fn output<I, S>(&self, args: I) -> Result<Output, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden(&mut command);
        command
            .output()
            .map_err(|error| format!("Failed to run Git: {error}"))
    }

    pub fn output_with_input<I, S>(&self, args: I, input: &[u8]) -> Result<Output, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(args);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden(&mut command);

        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to run Git: {error}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input)
                .map_err(|error| format!("Failed to send input to Git: {error}"))?;
        }
        child
            .wait_with_output()
            .map_err(|error| format!("Failed to read Git output: {error}"))
    }

    fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        match &self.backend {
            GitBackend::Native => {
                let mut command = Command::new("git");
                command
                    .arg("-C")
                    .arg(&self.repo_path)
                    .args(args)
                    .env("GIT_TERMINAL_PROMPT", "0")
                    .env("GIT_EDITOR", "true")
                    .env("GIT_SEQUENCE_EDITOR", "true")
                    .env("GIT_MERGE_AUTOEDIT", "no");
                command
            }
            GitBackend::Wsl { distro, linux_path } => {
                let mut command = Command::new("wsl.exe");
                command
                    .arg("-d")
                    .arg(distro)
                    .arg("--exec")
                    .arg("env")
                    .arg("GIT_TERMINAL_PROMPT=0")
                    .arg("GIT_EDITOR=true")
                    .arg("GIT_SEQUENCE_EDITOR=true")
                    .arg("GIT_MERGE_AUTOEDIT=no")
                    .arg("git")
                    .arg("-C")
                    .arg(linux_path)
                    .args(args);
                command
            }
        }
    }
}

pub fn configure_repo_backend(path: &str, backend: &str, distro: Option<&str>) {
    let backends = REPO_BACKENDS.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(mut entries) = backends.write() {
        entries.insert(
            normalized_key(Path::new(path)),
            (backend.to_string(), distro.map(str::to_string)),
        );
    }
}

pub fn validate_wsl_distro(distro: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("wsl.exe");
        command
            .arg("-d")
            .arg(distro)
            .arg("--exec")
            .arg("git")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        configure_hidden(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("Failed to start WSL: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("Could not run Git in WSL distribution '{distro}'")
        } else {
            error
        });
    }
    #[cfg(not(windows))]
    {
        let _ = distro;
        Err("WSL Git is only available on Windows".into())
    }
}

fn configured_backend(path: &Path) -> Option<GitBackend> {
    let entries = REPO_BACKENDS.get()?.read().ok()?;
    let (backend, distro) = entries.get(&normalized_key(path))?;
    match backend.as_str() {
        "native" => Some(GitBackend::Native),
        "wsl" => {
            let distro = distro.as_ref()?.trim();
            if distro.is_empty() {
                return None;
            }
            Some(GitBackend::Wsl {
                distro: distro.to_string(),
                linux_path: to_wsl_path(path)?,
            })
        }
        _ => None,
    }
}

fn normalized_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn to_wsl_path(path: &Path) -> Option<String> {
    if let GitBackend::Wsl { linux_path, .. } = detect_backend(path) {
        return Some(linux_path);
    }
    let display = path.to_string_lossy().replace('\\', "/");
    let bytes = display.as_bytes();
    if bytes.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        return Some(format!("/mnt/{drive}/{}", &display[3..]));
    }
    None
}

fn detect_backend(path: &Path) -> GitBackend {
    let display = path.to_string_lossy().replace('/', "\\");
    let without_prefix = display
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or(display);

    let lower = without_prefix.to_ascii_lowercase();
    let prefix_len = if lower.starts_with(r"\\wsl.localhost\") {
        r"\\wsl.localhost\".len()
    } else if lower.starts_with(r"\\wsl$\") {
        r"\\wsl$\".len()
    } else {
        return GitBackend::Native;
    };

    let rest = &without_prefix[prefix_len..];
    let mut parts = rest.split('\\').filter(|part| !part.is_empty());
    let Some(distro) = parts.next() else {
        return GitBackend::Native;
    };
    let linux_path = format!("/{}", parts.collect::<Vec<_>>().join("/"));

    GitBackend::Wsl {
        distro: distro.to_string(),
        linux_path,
    }
}

pub fn configure_hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_modern_wsl_unc_path() {
        let backend = detect_backend(Path::new(r"\\wsl.localhost\Ubuntu\home\person\project"));
        assert_eq!(
            backend,
            GitBackend::Wsl {
                distro: "Ubuntu".into(),
                linux_path: "/home/person/project".into(),
            }
        );
    }

    #[test]
    fn keeps_regular_paths_native() {
        assert_eq!(
            detect_backend(Path::new(r"C:\Users\person\project")),
            GitBackend::Native
        );
    }

    #[test]
    fn translates_windows_drive_path_for_wsl() {
        assert_eq!(
            to_wsl_path(Path::new(r"C:\Users\person\project")),
            Some("/mnt/c/Users/person/project".into())
        );
    }
}
