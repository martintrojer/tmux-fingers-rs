use std::collections::BTreeMap;
use std::env;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::tmux_style_printer::{Shell, TmuxStylePrinter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub pane_id: String,
    pub window_id: String,
    pub pane_width: i32,
    pub pane_height: i32,
    pub pane_current_path: String,
    pub pane_in_mode: bool,
    pub scroll_position: Option<i32>,
    pub window_zoomed_flag: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub window_id: String,
    pub window_width: i32,
    pub window_height: i32,
    pub pane_id: String,
    pub pane_tty: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TmuxVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Clone, Default)]
pub struct Tmux {
    version_override: Option<String>,
    socket_override: Option<String>,
    executed: Arc<Mutex<Vec<String>>>,
    fake_responses: Arc<Mutex<BTreeMap<String, String>>>,
}

impl Tmux {
    pub fn new() -> Self {
        Self {
            version_override: None,
            socket_override: env::var("FINGERS_TMUX_SOCKET").ok(),
            executed: Arc::new(Mutex::new(Vec::new())),
            fake_responses: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    pub fn fake(version: &str) -> Self {
        Self {
            version_override: Some(version.to_string()),
            socket_override: None,
            executed: Arc::new(Mutex::new(Vec::new())),
            fake_responses: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    pub fn fake_with_responses(
        version: &str,
        responses: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            version_override: Some(version.to_string()),
            socket_override: None,
            executed: Arc::new(Mutex::new(Vec::new())),
            fake_responses: Arc::new(Mutex::new(responses.into_iter().collect())),
        }
    }

    pub fn exec(&self, cmd: &str) -> Result<String, String> {
        if self.version_override.is_some() {
            self.executed.lock().unwrap().push(cmd.to_string());
            return Ok(self
                .fake_responses
                .lock()
                .unwrap()
                .get(cmd)
                .cloned()
                .unwrap_or_default());
        }

        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(format!("tmux{} {cmd}", self.socket_flag()))
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    pub fn version_string(&self) -> Result<String, String> {
        if let Some(version) = &self.version_override {
            return Ok(version.clone());
        }
        let output = Command::new("tmux")
            .args(self.socket_args())
            .arg("-V")
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .to_string())
    }

    pub fn show_option(&self, option: &str) -> Result<String, String> {
        self.exec(&format!("show-option -gv {}", shell_words::quote(option)))
    }

    pub fn fingers_option_names(&self) -> Result<Vec<String>, String> {
        if self.version_override.is_some() {
            let output = self
                .fake_responses
                .lock()
                .unwrap()
                .get("__fingers_option_names__")
                .cloned()
                .unwrap_or_default();
            return Ok(output
                .lines()
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect());
        }
        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(format!(
                "tmux{} show-options -g | grep ^@fingers",
                self.socket_flag()
            ))
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() && !output.stdout.is_empty() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(ToOwned::to_owned))
            .collect())
    }

    pub fn parse_style(&self, style: &str) -> Result<String, String> {
        let mut printer = TmuxStylePrinter::new(TputShell);
        printer.print(style, false)
    }

    pub fn set_buffer(&self, value: &str, use_system_clipboard: bool) -> Result<(), String> {
        if self.version_override.is_some() {
            self.executed
                .lock()
                .unwrap()
                .push(format!("buffer:{use_system_clipboard}:{value}"));
            return Ok(());
        }

        let version = tmux_version_to_semver(&self.version_string()?)?;
        let mut args = vec!["load-buffer".to_string()];
        if version >= tmux_version_to_semver("3.2")? && use_system_clipboard {
            args.push("-w".to_string());
        }
        args.push("-".to_string());

        let mut child = Command::new("tmux")
            .args(self.socket_args())
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| err.to_string())?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(value.as_bytes())
                .map_err(|err| err.to_string())?;
        }
        let output = child.wait_with_output().map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn executed_commands(&self) -> Vec<String> {
        self.executed.lock().unwrap().clone()
    }

    fn socket_args(&self) -> Vec<String> {
        self.socket_override
            .as_deref()
            .map(shell_words::split)
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn socket_flag(&self) -> String {
        self.socket_override
            .as_ref()
            .map(|flag| format!(" {}", flag))
            .unwrap_or_default()
    }

    pub fn display_message(&self, message: &str, delay: i32) -> Result<(), String> {
        self.exec(&format!(
            "display-message -d {} {}",
            delay,
            shell_words::quote(message)
        ))?;
        Ok(())
    }

    pub fn set_global_option(&self, name: &str, value: &str) -> Result<(), String> {
        self.exec(&format!(
            "set-option -g {} {}",
            name,
            shell_words::quote(value)
        ))?;
        Ok(())
    }

    pub fn disable_prefix(&self) -> Result<(), String> {
        self.set_global_option("prefix", "None")?;
        self.set_global_option("prefix2", "None")
    }

    pub fn set_key_table(&self, table: &str) -> Result<(), String> {
        self.exec(&format!(
            "set-window-option key-table {}",
            shell_words::quote(table)
        ))?;
        self.exec(&format!("switch-client -T {}", shell_words::quote(table)))?;
        Ok(())
    }

    pub fn select_pane(&self, pane_id: &str) -> Result<(), String> {
        let mut args = format!("select-pane -t {}", shell_words::quote(pane_id));
        if tmux_version_to_semver(&self.version_string()?)? >= tmux_version_to_semver("3.1")? {
            args.push_str(" -Z");
        }
        self.exec(&args)?;
        Ok(())
    }

    pub fn swap_panes(&self, src_id: &str, dst_id: &str) -> Result<(), String> {
        let mut args = format!(
            "swap-pane -d -s {} -t {}",
            shell_words::quote(src_id),
            shell_words::quote(dst_id)
        );
        if tmux_version_to_semver(&self.version_string()?)? >= tmux_version_to_semver("3.1")? {
            args.push_str(" -Z");
        }
        self.exec(&args)?;
        Ok(())
    }

    pub fn kill_pane(&self, pane_id: &str) -> Result<(), String> {
        self.exec(&format!("kill-pane -t {}", shell_words::quote(pane_id)))?;
        Ok(())
    }

    pub fn resize_window(&self, window_id: &str, width: i32, height: i32) -> Result<(), String> {
        self.exec(&format!(
            "resize-window -t {} -x {} -y {}",
            shell_words::quote(window_id),
            width,
            height
        ))?;
        Ok(())
    }

    pub fn capture_pane(&self, pane: &Pane, join: bool) -> Result<String, String> {
        let join_flag = if join { "-J " } else { "" };
        if pane.pane_in_mode
            && let Some(scroll_position) = pane.scroll_position
        {
            let start_line = -scroll_position;
            let end_line = pane.pane_height - scroll_position - 1;
            return self.exec(&format!(
                "capture-pane {join_flag}-p -t {} -S {} -E {}",
                shell_words::quote(&pane.pane_id),
                start_line,
                end_line
            ));
        }
        self.exec(&format!(
            "capture-pane {join_flag}-p -t {}",
            shell_words::quote(&pane.pane_id)
        ))
    }

    pub fn create_window(
        &self,
        name: &str,
        cmd: &str,
        _pane_width: i32,
        _pane_height: i32,
    ) -> Result<Window, String> {
        let output = self.exec(&format!(
            "new-window -c '#{{pane_current_path}}' -P -d -n {} -F '#{{window_id}};#{{window_width}};#{{window_height}};#{{pane_id}};#{{pane_tty}}' {}",
            shell_words::quote(name),
            shell_words::quote(cmd)
        ))?;
        parse_window(&output)
    }

    pub fn find_pane_by_id(&self, id: &str) -> Result<Option<Pane>, String> {
        let output = self.exec(&format!(
            "display-message -t {} -F '#{{pane_id}};#{{window_id}};#{{pane_width}};#{{pane_height}};#{{pane_current_path}};#{{?pane_in_mode,true,false}};#{{?scroll_position,#{{scroll_position}},}};#{{?window_zoomed_flag,true,false}}' -p",
            shell_words::quote(id)
        ))?;
        if output.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(parse_pane(&output)?))
    }

    pub fn list_panes(&self, filter: &str, target: &str) -> Result<Vec<Pane>, String> {
        let output = self.exec(&format!(
            "list-panes -F '#{{pane_id}};#{{window_id}};#{{pane_width}};#{{pane_height}};#{{pane_current_path}};#{{?pane_in_mode,true,false}};#{{?scroll_position,#{{scroll_position}},}};#{{?window_zoomed_flag,true,false}}' -t {} -f {}",
            shell_words::quote(target),
            shell_words::quote(filter)
        ))?;
        output.lines().map(parse_pane).collect()
    }
}

struct TputShell;

impl Shell for TputShell {
    fn exec(&self, cmd: &str) -> Result<String, String> {
        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(cmd)
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

pub fn tmux_version_to_semver(input: &str) -> Result<TmuxVersion, String> {
    let mut chars = input.chars().peekable();

    let major = parse_number(&mut chars).ok_or_else(|| format!("Invalid tmux version {input}"))?;
    if chars.next() != Some('.') {
        return Err(format!("Invalid tmux version {input}"));
    }
    let minor = parse_number(&mut chars).ok_or_else(|| format!("Invalid tmux version {input}"))?;
    let patch = match chars.next() {
        None => 0,
        Some(letter) if letter.is_ascii_lowercase() && chars.next().is_none() => {
            u32::from(letter as u8 - b'a' + 1)
        }
        _ => return Err(format!("Invalid tmux version {input}")),
    };

    Ok(TmuxVersion {
        major,
        minor,
        patch,
    })
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<u32> {
    let mut out = String::new();
    while let Some(ch) = chars.peek() {
        if !ch.is_ascii_digit() {
            break;
        }
        out.push(*ch);
        chars.next();
    }
    if out.is_empty() {
        None
    } else {
        out.parse().ok()
    }
}

fn parse_pane(input: &str) -> Result<Pane, String> {
    let parts = input.trim_end().split(';').collect::<Vec<_>>();
    if parts.len() != 8 {
        return Err(format!("Invalid pane output: {input}"));
    }
    Ok(Pane {
        pane_id: parts[0].to_string(),
        window_id: parts[1].to_string(),
        pane_width: parts[2]
            .parse()
            .map_err(|_| format!("Invalid pane width: {input}"))?,
        pane_height: parts[3]
            .parse()
            .map_err(|_| format!("Invalid pane height: {input}"))?,
        pane_current_path: parts[4].to_string(),
        pane_in_mode: parts[5] == "true",
        scroll_position: if parts[6].is_empty() {
            None
        } else {
            Some(
                parts[6]
                    .parse()
                    .map_err(|_| format!("Invalid scroll position: {input}"))?,
            )
        },
        window_zoomed_flag: parts[7] == "true",
    })
}

fn parse_window(input: &str) -> Result<Window, String> {
    let parts = input.trim_end().split(';').collect::<Vec<_>>();
    if parts.len() != 5 {
        return Err(format!("Invalid window output: {input}"));
    }
    Ok(Window {
        window_id: parts[0].to_string(),
        window_width: parts[1]
            .parse()
            .map_err(|_| format!("Invalid window width: {input}"))?,
        window_height: parts[2]
            .parse()
            .map_err(|_| format!("Invalid window height: {input}"))?,
        pane_id: parts[3].to_string(),
        pane_tty: parts[4].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::tmux_version_to_semver;

    #[test]
    fn parses_plain_versions() {
        let version = tmux_version_to_semver("3.1").unwrap();
        assert_eq!(version.major, 3);
        assert_eq!(version.minor, 1);
        assert_eq!(version.patch, 0);
    }

    #[test]
    fn parses_letter_versions() {
        let version = tmux_version_to_semver("3.1b").unwrap();
        assert_eq!(version.major, 3);
        assert_eq!(version.minor, 1);
        assert_eq!(version.patch, 2);
    }

    #[test]
    fn compares_versions() {
        let left = tmux_version_to_semver("3.0a").unwrap();
        let right = tmux_version_to_semver("3.1").unwrap();
        assert!(left < right);
    }
}
