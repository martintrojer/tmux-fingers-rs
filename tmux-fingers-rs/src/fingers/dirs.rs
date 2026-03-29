use std::env;
use std::fs;
use std::path::PathBuf;

pub fn root_dir() -> PathBuf {
    let tmux_pid = env::var("TMUX")
        .ok()
        .and_then(|value| value.split(',').nth(1).map(ToOwned::to_owned))
        .unwrap_or_else(|| "0000".to_string());
    state_root().join(format!("tmux-{tmux_pid}"))
}

pub fn log_path() -> PathBuf {
    if let Ok(path) = env::var("FINGERS_LOG_PATH") {
        return PathBuf::from(path);
    }
    state_root().join("fingers.log")
}

pub fn socket_path() -> PathBuf {
    root_dir().join("fingers.sock")
}

pub fn config_path() -> PathBuf {
    root_dir().join("config.json")
}

pub fn ensure_folders() -> std::io::Result<()> {
    fs::create_dir_all(root_dir())
}

fn state_root() -> PathBuf {
    if let Ok(path) = env::var("XDG_STATE_HOME") {
        return PathBuf::from(path).join("tmux-fingers");
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/state/tmux-fingers")
}
