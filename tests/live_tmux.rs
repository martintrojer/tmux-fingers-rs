use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

/// Returns a short, unique base directory for per-test state.
///
/// We deliberately avoid `std::env::temp_dir()` here. On macOS that resolves
/// to a long `/var/folders/...` path, and the resulting unix socket path
/// (`<state>/tmux-fingers-rs/tmux-0000/fingers.sock`) easily exceeds the
/// 104-byte `SUN_LEN` limit, causing tmux to fail with
/// `path must be shorter than SUN_LEN`.
fn short_state_home() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Keep the prefix tiny so the full socket path stays well under 104 bytes.
    // e.g. /tmp/tf-<pid>-<nanos-suffix>
    let suffix = (nanos % 1_000_000) as u32;
    PathBuf::from("/tmp").join(format!("tf-{}-{suffix:06}", std::process::id()))
}

fn tmux(socket: &str, args: &[&str]) -> String {
    let output = Command::new("tmux")
        .arg("-L")
        .arg(socket)
        .args(args)
        .output()
        .expect("run tmux");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn binary() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_tmux-fingers-rs") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("current exe");
    exe.parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("tmux-fingers-rs"))
        .expect("compiled binary path")
}

fn setup_server(socket: &str, session: &str, command: &str) {
    let output = Command::new("tmux")
        .arg("-L")
        .arg(socket)
        .arg("-f")
        .arg("/dev/null")
        .args(["new-session", "-d", "-s", session, command])
        .output()
        .expect("start tmux server");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn attach_control_client(socket: &str, session: &str) -> Child {
    Command::new("tmux")
        .arg("-L")
        .arg(socket)
        .arg("-C")
        .args(["attach-session", "-t", session])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("attach control client")
}

fn spawn_binary(bin: &Path, state_home: &Path, socket: &str, args: &[&str]) -> Child {
    Command::new(bin)
        .args(args)
        .env("XDG_STATE_HOME", state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .spawn()
        .expect("spawn binary")
}

fn run_load_config(bin: &Path, state_home: &Path, socket: &str) {
    let load = Command::new(bin)
        .arg("load-config")
        .env("XDG_STATE_HOME", state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run load-config");
    assert!(
        load.status.success(),
        "{}",
        String::from_utf8_lossy(&load.stderr)
    );
}

fn socket_path(state_home: &Path) -> PathBuf {
    state_home
        .join("tmux-fingers-rs")
        .join("tmux-0000")
        .join("fingers.sock")
}

fn wait_for_socket(socket_path: &Path) {
    for _ in 0..50 {
        if socket_path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("socket not created: {}", socket_path.display());
}

fn cleanup(socket: &str, mut client: Child, state_home: &Path) {
    let _ = client.kill();
    let _ = client.wait();
    let _ = Command::new("tmux")
        .arg("-L")
        .arg(socket)
        .arg("kill-server")
        .status();
    let _ = fs::remove_dir_all(state_home);
}

#[test]
fn load_config_and_start_work_against_live_tmux() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-enable-bindings", "1"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(&bin, &state_home, &socket, &["start", &pane_id]);

    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    let status = start.wait().expect("wait for start");
    assert!(status.success());

    assert_eq!(tmux(&socket, &["show-buffer"]), "12345");
    let windows = tmux(&socket, &["list-windows", "-F", "#{window_name}"]);
    assert!(!windows.lines().any(|name| name == "[fingers]"));

    cleanup(&socket, client, &state_home);
}

#[test]
fn multimode_selects_multiple_matches() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345 67890\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(&bin, &state_home, &socket, &["start", &pane_id]);
    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    for input in [
        "toggle-multi-mode",
        "hint:b:main",
        "hint:y:main",
        "toggle-multi-mode",
    ] {
        let send = Command::new(&bin)
            .args(["send-input", input])
            .env("XDG_STATE_HOME", &state_home)
            .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
            .output()
            .expect("run send-input");
        assert!(
            send.status.success(),
            "{}",
            String::from_utf8_lossy(&send.stderr)
        );
    }

    assert!(start.wait().expect("wait for start").success());
    assert_eq!(tmux(&socket, &["show-buffer"]), "12345 67890");

    cleanup(&socket, client, &state_home);
}

#[test]
fn jump_mode_enters_copy_mode_on_selection() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(
        &bin,
        &state_home,
        &socket,
        &["start", "--mode", "jump", &pane_id],
    );
    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    assert!(start.wait().expect("wait for start").success());
    let pane_in_mode = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{?pane_in_mode,1,0}",
        ],
    );
    assert_eq!(pane_in_mode, "1");

    cleanup(&socket, client, &state_home);
}

#[test]
fn custom_pattern_is_loaded_and_selected() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(
        &socket,
        &session,
        "printf 'deploy abc-123 done\n'; exec cat",
    );
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &["set-option", "-g", "@fingers-enabled-builtin-patterns", ""],
    );
    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-pattern-0",
            "deploy (?<match>abc-123)",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(&bin, &state_home, &socket, &["start", &pane_id]);
    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    assert!(start.wait().expect("wait for start").success());
    assert_eq!(tmux(&socket, &["show-buffer"]), "abc-123");

    cleanup(&socket, client, &state_home);
}

#[test]
fn paste_action_pastes_match_into_pane() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(
        &bin,
        &state_home,
        &socket,
        &["start", "--main-action", ":paste:", &pane_id],
    );
    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    assert!(start.wait().expect("wait for start").success());
    thread::sleep(Duration::from_millis(100));
    let pane_text = tmux(
        &socket,
        &["capture-pane", "-p", "-t", &format!("{session}:0.0")],
    );
    assert!(
        pane_text.contains("12345\n12345"),
        "pane_text={pane_text:?}"
    );

    cleanup(&socket, client, &state_home);
}

#[test]
fn custom_shell_action_receives_match_on_stdin() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();
    let output_path = state_home.join("action-output.txt");

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let shell_action = format!("/bin/sh -lc 'cat > {}'", output_path.display());
    let mut start = spawn_binary(
        &bin,
        &state_home,
        &socket,
        &["start", "--main-action", &shell_action, &pane_id],
    );
    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    assert!(start.wait().expect("wait for start").success());
    for _ in 0..20 {
        if output_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let written = fs::read_to_string(&output_path).expect("action output");
    assert_eq!(written, "12345");

    cleanup(&socket, client, &state_home);
}

#[test]
fn failed_start_still_restores_tmux_state() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(
        &socket,
        &[
            "set-option",
            "-g",
            "@fingers-enabled-builtin-patterns",
            "digit",
        ],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-use-system-clipboard", "0"],
    );
    tmux(
        &socket,
        &["set-option", "-g", "@fingers-show-copied-notification", "0"],
    );
    tmux(&socket, &["set-option", "-g", "prefix", "C-a"]);
    tmux(&socket, &["set-option", "-g", "prefix2", "C-Space"]);

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let pane_id = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0.0"),
            "#{pane_id}",
        ],
    );
    let mut start = spawn_binary(
        &bin,
        &state_home,
        &socket,
        &[
            "start",
            "--main-action",
            "/definitely/missing/tmux-fingers-bin",
            &pane_id,
        ],
    );

    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = Command::new(&bin)
        .args(["send-input", "hint:b:main"])
        .env("XDG_STATE_HOME", &state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"))
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    let status = start.wait().expect("wait for start");
    assert!(!status.success());

    let windows = tmux(&socket, &["list-windows", "-F", "#{window_name}"]);
    assert!(!windows.lines().any(|name| name == "[fingers]"));

    let client_state = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "#{client_key_table};#{prefix};#{prefix2}",
        ],
    );
    assert_eq!(client_state, "root;C-a;C-Space");

    cleanup(&socket, client, &state_home);
}
