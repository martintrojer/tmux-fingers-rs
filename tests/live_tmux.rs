use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

/// Per-process sequence for unique names. Not the clock: macOS `SystemTime`
/// has only microsecond resolution, so clock-derived names collided when
/// tests started together under the parallel harness (duplicate tmux
/// sessions, or two tests sharing one fingers socket so a `start` waited
/// forever for input).
fn next_id() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn unique_name(prefix: &str) -> String {
    format!("{prefix}-{}-{}", std::process::id(), next_id())
}

/// Returns a short, unique base directory for per-test state.
///
/// We deliberately avoid `std::env::temp_dir()` here. On macOS that resolves
/// to a long `/var/folders/...` path, and the resulting unix socket path
/// (`<state>/tmux-fingers-rs/tmux-0000/fingers.sock`) easily exceeds the
/// 104-byte `SUN_LEN` limit, causing tmux to fail with
/// `path must be shorter than SUN_LEN`.
fn short_state_home() -> PathBuf {
    // Keep the prefix tiny so the full socket path stays well under 104 bytes.
    // e.g. /tmp/tf-<pid>-<n>
    PathBuf::from("/tmp").join(format!("tf-{}-{}", std::process::id(), next_id()))
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

/// A `Command` for the binary under test, pointed at the test's tmux socket
/// and state dir.
///
/// `TMUX` / `TMUX_PANE` are cleared: the binary derives its state dir from
/// the server pid in `$TMUX` (`tmux-<pid>`), so when the suite runs inside a
/// tmux session it would otherwise listen on the outer server's socket path
/// instead of the `tmux-0000` one these tests wait for.
fn fingers(bin: &Path, state_home: &Path, socket: &str) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env("XDG_STATE_HOME", state_home)
        .env("FINGERS_TMUX_SOCKET", format!("-L {socket}"));
    cmd
}

/// Kills the wrapped `start` process on drop. Without this a failing test
/// orphans `start`, which keeps the test's stdout/stderr open and makes a
/// piped `cargo test` hang instead of reporting the failure.
struct KillOnDrop(Child);

impl std::ops::Deref for KillOnDrop {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl std::ops::DerefMut for KillOnDrop {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_binary(bin: &Path, state_home: &Path, socket: &str, args: &[&str]) -> KillOnDrop {
    KillOnDrop(
        fingers(bin, state_home, socket)
            .args(args)
            .spawn()
            .expect("spawn binary"),
    )
}

fn run_load_config(bin: &Path, state_home: &Path, socket: &str) {
    let load = fingers(bin, state_home, socket)
        .arg("load-config")
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

    let fingers_keys = tmux(&socket, &["list-keys", "-T", "fingers"]);
    assert!(
        fingers_keys.contains("send-input hint:a:main"),
        "{fingers_keys}"
    );
    let prefix_keys = tmux(&socket, &["list-keys", "-T", "prefix"]);
    assert!(
        prefix_keys.lines().any(|line| {
            line.starts_with("bind-key ") && line.contains(" F ") && line.contains(" start ")
        }),
        "{prefix_keys}"
    );
    assert_eq!(
        tmux(&socket, &["show-option", "-gv", "@fingers-cli"]),
        bin.to_string_lossy()
    );

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

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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
fn echoing_login_profile_does_not_break_load_config_or_start() {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    let home = state_home.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join(".profile"), "echo hello-from-profile\n").unwrap();

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
    let load = fingers(&bin, &state_home, &socket)
        .env("HOME", &home)
        .arg("load-config")
        .output()
        .expect("run load-config");
    assert!(
        load.status.success(),
        "{}",
        String::from_utf8_lossy(&load.stderr)
    );

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
    let mut start = KillOnDrop(
        fingers(&bin, &state_home, &socket)
            .env("HOME", &home)
            .args(["start", &pane_id])
            .spawn()
            .expect("spawn binary"),
    );
    wait_for_socket(&socket_path(&state_home));

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );
    assert!(start.wait().expect("wait for start").success());
    assert_eq!(tmux(&socket, &["show-buffer"]), "12345");

    cleanup(&socket, client, &state_home);
}

#[test]
fn invalid_root_key_does_not_disable_other_bindings() {
    let socket = unique_name("tmux-fingers-rs-invalid-key");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));
    tmux(&socket, &["set-option", "-g", "@fingers-key", "NotAKey"]);

    let bin = binary();
    let load = fingers(&bin, &state_home, &socket)
        .arg("load-config")
        .output()
        .expect("run load-config");
    assert!(!load.status.success());
    assert!(
        String::from_utf8_lossy(&load.stderr).contains("unknown key: NotAKey"),
        "{}",
        String::from_utf8_lossy(&load.stderr)
    );

    let fingers_keys = tmux(&socket, &["list-keys", "-T", "fingers"]);
    assert!(fingers_keys.contains("send-input"), "{fingers_keys}");
    let prefix_keys = tmux(&socket, &["list-keys", "-T", "prefix"]);
    assert!(prefix_keys.contains("start --mode jump"), "{prefix_keys}");
    assert_eq!(
        tmux(&socket, &["show-option", "-gv", "@fingers-cli"]),
        bin.to_string_lossy()
    );

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
        let send = fingers(&bin, &state_home, &socket)
            .args(["send-input", input])
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

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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
fn paste_action_cancels_copy_mode_before_pasting() {
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
    tmux(&socket, &["copy-mode", "-t", &pane_id]);
    let mut start = spawn_binary(
        &bin,
        &state_home,
        &socket,
        &["start", "--main-action", ":paste:", &pane_id],
    );
    wait_for_socket(&socket_path(&state_home));

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
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
fn render_error_preserves_original_pane_and_tmux_state() {
    assert_render_error_preserves_original_pane_and_tmux_state(false);
}

#[test]
fn render_error_preserves_zoomed_pane_and_tmux_state() {
    assert_render_error_preserves_original_pane_and_tmux_state(true);
}

fn assert_render_error_preserves_original_pane_and_tmux_state(zoomed: bool) {
    let socket = unique_name("tmux-fingers-rs");
    let session = unique_name("session");
    let state_home = short_state_home();
    fs::create_dir_all(&state_home).unwrap();

    setup_server(&socket, &session, "printf '12345\n'; exec cat");
    let client = attach_control_client(&socket, &session);
    thread::sleep(Duration::from_millis(200));

    tmux(&socket, &["set-option", "-g", "prefix", "C-a"]);
    tmux(&socket, &["set-option", "-g", "prefix2", "C-Space"]);

    let bin = binary();
    run_load_config(&bin, &state_home, &socket);

    let config_path = state_home
        .join("tmux-fingers-rs")
        .join("tmux-0000")
        .join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["patterns"] = serde_json::json!({"bad": "(unclosed"});
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

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
    if zoomed {
        tmux(&socket, &["split-window", "-d", "-t", &pane_id, "exec cat"]);
        tmux(&socket, &["resize-pane", "-Z", "-t", &pane_id]);
        assert_eq!(
            tmux(
                &socket,
                &[
                    "display-message",
                    "-p",
                    "-t",
                    &pane_id,
                    "#{window_zoomed_flag}"
                ],
            ),
            "1"
        );
    }

    let original_layout = tmux(
        &socket,
        &[
            "list-panes",
            "-a",
            "-F",
            "#{window_id};#{window_layout};#{pane_id}",
        ],
    );
    let original_client_state = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "#{client_key_table};#{prefix};#{prefix2}",
        ],
    );

    let start = fingers(&bin, &state_home, &socket)
        .args(["start", &pane_id])
        .output()
        .expect("run start");
    let stderr = String::from_utf8_lossy(&start.stderr).into_owned();
    let final_layout = tmux(
        &socket,
        &[
            "list-panes",
            "-a",
            "-F",
            "#{window_id};#{window_layout};#{pane_id}",
        ],
    );
    let windows = tmux(&socket, &["list-windows", "-F", "#{window_name}"]);
    let final_client_state = tmux(
        &socket,
        &[
            "display-message",
            "-p",
            "#{client_key_table};#{prefix};#{prefix2}",
        ],
    );
    let saved_config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();

    cleanup(&socket, client, &state_home);

    assert!(!start.status.success(), "start unexpectedly succeeded");
    assert!(
        stderr.contains("missing closing parenthesis"),
        "expected pattern compilation error, got: {stderr}"
    );
    assert_eq!(final_layout, original_layout);
    assert!(!windows.lines().any(|name| name == "[fingers]"));
    assert_eq!(final_client_state, original_client_state);
    assert_eq!(final_client_state, "root;C-a;C-Space");
    assert_eq!(
        saved_config["patterns"],
        serde_json::json!({"bad": "(unclosed"})
    );
}

#[test]
fn failed_action_is_reported_and_still_restores_tmux_state() {
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
    // Since upstream 2.7.1 (`add error handling and reporting when running
    // actions`), a failing action is reported rather than aborting the run, so
    // `start` exits 0 and teardown still restores tmux state.
    let mut start = KillOnDrop(
        fingers(&bin, &state_home, &socket)
            .args([
                "start",
                "--main-action",
                "/definitely/missing/tmux-fingers-bin",
                &pane_id,
            ])
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn binary"),
    );
    let mut start_stderr = start.stderr.take().expect("piped stderr");

    let socket_path = socket_path(&state_home);
    wait_for_socket(&socket_path);

    let send = fingers(&bin, &state_home, &socket)
        .args(["send-input", "hint:b:main"])
        .output()
        .expect("run send-input");
    assert!(
        send.status.success(),
        "{}",
        String::from_utf8_lossy(&send.stderr)
    );

    let status = start.wait().expect("wait for start");
    assert!(status.success(), "start should not abort on action failure");

    let mut stderr = String::new();
    start_stderr
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert!(
        stderr.contains("[tmux-fingers-rs] error processing result:"),
        "expected the action failure to be reported, got: {stderr}"
    );

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
