use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::fingers::config::Config;
use crate::fingers::dirs;
use crate::tmux::Tmux;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneInfo {
    pub pane_id: String,
    pub pane_current_path: String,
    pub pane_in_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ActionRunner {
    pub modifier: String,
    pub r#match: String,
    pub hint: String,
    pub original_pane: PaneInfo,
    pub offset: Option<(usize, usize)>,
    pub mode: String,
    pub main_action: Option<String>,
    pub ctrl_action: Option<String>,
    pub alt_action: Option<String>,
    pub shift_action: Option<String>,
}

impl ActionRunner {
    pub fn final_shell_command(&self, config: &Config) -> Option<String> {
        if self.mode == "jump" {
            return None;
        }

        match self.action(config).as_deref() {
            Some(":copy:") => self.system_copy_command(config),
            Some(":open:") => self.system_open_command(),
            Some(":paste:") => Some(self.paste_command()),
            Some("") | None => None,
            Some(other) => Some(other.to_string()),
        }
    }

    pub fn run(&self, config: &Config, tmux: &Tmux) -> Result<(), String> {
        tmux.set_buffer(&self.r#match, config.use_system_clipboard)?;

        if self.mode == "jump" {
            if let Some((row, col)) = self.offset {
                tmux.exec(&format!("copy-mode -t {}", self.original_pane.pane_id))?;
                tmux.exec(&format!(
                    "send-keys -t {} -X top-line",
                    self.original_pane.pane_id
                ))?;
                if row > 0 {
                    tmux.exec(&format!(
                        "send-keys -t {} -N {} -X cursor-down",
                        self.original_pane.pane_id, row
                    ))?;
                }
                if col > 0 {
                    tmux.exec(&format!(
                        "send-keys -t {} -N {} -X cursor-right",
                        self.original_pane.pane_id, col
                    ))?;
                }
            }
            return Ok(());
        }

        if self.action(config).as_deref() == Some(":paste:") {
            tmux.exec(&self.paste_command())?;
            return Ok(());
        }

        let Some(command) = self.final_shell_command(config) else {
            return Ok(());
        };

        let parts = shell_words::split(&command).map_err(|err| err.to_string())?;
        let (program, args) = parts
            .split_first()
            .ok_or_else(|| "empty action command".to_string())?;
        let stderr_path = dirs::root_dir().join("action-stderr");
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(stderr_path)
            .map_err(|err| err.to_string())?;
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .env("MODIFIER", &self.modifier)
            .env("HINT", &self.hint);

        if !self.original_pane.pane_current_path.is_empty() {
            command.current_dir(&self.original_pane.pane_current_path);
        }

        let mut child = command.spawn().map_err(|err| err.to_string())?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(self.expanded_match(config).as_bytes())
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    fn action(&self, config: &Config) -> Option<String> {
        match self.modifier.as_str() {
            "main" => self
                .main_action
                .clone()
                .or_else(|| Some(config.main_action.clone())),
            "shift" => self
                .shift_action
                .clone()
                .or_else(|| Some(config.shift_action.clone())),
            "alt" => self
                .alt_action
                .clone()
                .or_else(|| Some(config.alt_action.clone())),
            "ctrl" => self
                .ctrl_action
                .clone()
                .or_else(|| Some(config.ctrl_action.clone())),
            _ => None,
        }
    }

    fn paste_command(&self) -> String {
        if self.original_pane.pane_in_mode {
            format!(
                "send-keys -t {} -X cancel ; paste-buffer -t {}",
                self.original_pane.pane_id, self.original_pane.pane_id
            )
        } else {
            format!("paste-buffer -t {}", self.original_pane.pane_id)
        }
    }

    fn system_copy_command(&self, config: &Config) -> Option<String> {
        self.system_copy_command_with(config, command_exists)
    }

    fn system_open_command(&self) -> Option<String> {
        system_open_command_with(command_exists)
    }

    fn system_copy_command_with<F>(&self, config: &Config, command_exists: F) -> Option<String>
    where
        F: Fn(&str) -> bool,
    {
        if !config.use_system_clipboard {
            return None;
        }

        if command_exists("pbcopy") {
            if command_exists("reattach-to-user-namespace") {
                Some("reattach-to-user-namespace".to_string())
            } else {
                Some("pbcopy".to_string())
            }
        } else if command_exists("clip.exe") {
            Some("clip.exe".to_string())
        } else if command_exists("wl-copy") {
            Some("wl-copy".to_string())
        } else if command_exists("xclip") {
            Some("xclip -selection clipboard".to_string())
        } else if command_exists("xsel") {
            Some("xsel -i --clipboard".to_string())
        } else if command_exists("putclip") {
            Some("putclip".to_string())
        } else {
            None
        }
    }
}

fn system_open_command_with<F>(command_exists: F) -> Option<String>
where
    F: Fn(&str) -> bool,
{
    if command_exists("cygstart") {
        Some("xargs cygstart".to_string())
    } else if command_exists("xdg-open") {
        Some("xargs xdg-open".to_string())
    } else if command_exists("open") {
        Some("xargs open".to_string())
    } else {
        None
    }
}

impl ActionRunner {
    fn expanded_match(&self, config: &Config) -> String {
        if self.action(config).as_deref() == Some(":open:")
            && self.r#match.starts_with('~')
            && let Ok(home) = env::var("HOME")
        {
            let stripped = self.r#match.trim_start_matches("~/");
            return PathBuf::from(home)
                .join(stripped)
                .to_string_lossy()
                .into_owned();
        }
        self.r#match.clone()
    }
}

fn command_exists(program: &str) -> bool {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|path| path.join(program))
        .any(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::{ActionRunner, PaneInfo};
    use crate::fingers::config::Config;

    fn runner() -> ActionRunner {
        ActionRunner {
            modifier: "main".into(),
            r#match: "~/tmp/file.txt".into(),
            hint: "a".into(),
            original_pane: PaneInfo {
                pane_id: "%1".into(),
                pane_current_path: "/tmp".into(),
                pane_in_mode: false,
            },
            offset: Some((2, 3)),
            mode: "default".into(),
            main_action: None,
            ctrl_action: None,
            alt_action: None,
            shift_action: None,
        }
    }

    #[test]
    fn resolves_paste_command() {
        let config = Config {
            main_action: ":paste:".into(),
            ..Config::default()
        };
        let command = runner().final_shell_command(&config).unwrap();
        assert_eq!(command, "paste-buffer -t %1");
    }

    #[test]
    fn expands_home_for_open_action() {
        let config = Config {
            main_action: ":open:".into(),
            ..Config::default()
        };
        let expanded = runner().expanded_match(&config);
        assert!(expanded.ends_with("tmp/file.txt"));
    }

    #[test]
    fn resolves_windows_clipboard_command_without_shell_pipeline() {
        let config = Config {
            main_action: ":copy:".into(),
            ..Config::default()
        };
        let command = runner()
            .system_copy_command_with(&config, |name| name == "clip.exe")
            .unwrap();
        assert_eq!(command, "clip.exe");
    }

    #[test]
    fn resolves_open_action_to_platform_launcher() {
        let command = if cfg!(target_os = "macos") {
            super::system_open_command_with(|name| name == "open").unwrap()
        } else {
            super::system_open_command_with(|name| name == "xdg-open").unwrap()
        };
        let expected = if cfg!(target_os = "macos") {
            "xargs open"
        } else {
            "xargs xdg-open"
        };
        assert_eq!(command, expected);
    }
}
