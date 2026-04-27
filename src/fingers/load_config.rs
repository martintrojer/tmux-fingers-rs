use std::collections::BTreeMap;

use pcre2::bytes::Regex;

use crate::fingers::config::{Config, alphabet_map, builtin_patterns};
use crate::tmux::Tmux;

const PRIVATE_OPTIONS: &[&str] = &["skip_wizard", "cli"];
const DISALLOWED_CHARS: &[char] = &['c', 'i', 'm', 'q', 'n'];

pub fn run_load_config(tmux: &Tmux) -> Result<Config, String> {
    let option_names = tmux.fingers_option_names()?;
    validate_options(&option_names, tmux)?;
    let options = shell_safe_options(tmux, &option_names)?;
    let config = parse_options(options, tmux)?;
    config.save().map_err(|err| err.to_string())?;
    setup_bindings(tmux, &config)?;
    Ok(config)
}

pub fn parse_options(options: BTreeMap<String, String>, tmux: &Tmux) -> Result<Config, String> {
    let mut config = Config {
        tmux_version: tmux.version_string()?,
        ..Config::default()
    };

    let mut user_defined_patterns = Vec::new();
    for (option, value) in options {
        if option.starts_with("pattern_") && !value.is_empty() {
            check_pattern(&value)?;
            user_defined_patterns.push((
                option.trim_start_matches("pattern_").to_string(),
                value.clone(),
            ));
        }

        match option.as_str() {
            "key" => config.key = value,
            "jump_key" => config.jump_key = value,
            "keyboard_layout" => config.keyboard_layout = value,
            "main_action" => config.main_action = value,
            "ctrl_action" => config.ctrl_action = value,
            "alt_action" => config.alt_action = value,
            "shift_action" => config.shift_action = value,
            "use_system_clipboard" => config.use_system_clipboard = value == "1",
            "benchmark_mode" => config.benchmark_mode = value,
            "hint_position" => config.hint_position = value,
            "hint_style" => config.hint_style = tmux.parse_style(&value)?,
            "selected_hint_style" => config.selected_hint_style = tmux.parse_style(&value)?,
            "highlight_style" => config.highlight_style = tmux.parse_style(&value)?,
            "backdrop_style" => config.backdrop_style = tmux.parse_style(&value)?,
            "selected_highlight_style" => {
                config.selected_highlight_style = tmux.parse_style(&value)?
            }
            "show_copied_notification" => config.show_copied_notification = value,
            "enabled_builtin_patterns" => config.enabled_builtin_patterns = value,
            "enable_bindings" => config.enable_bindings = value == "1",
            _ => {}
        }
    }

    for (name, pattern) in user_defined_patterns {
        config.patterns.insert(name, pattern);
    }

    let pattern_names: Vec<String> = if config.enabled_builtin_patterns == "all" {
        builtin_patterns()
            .keys()
            .map(|name| (*name).to_string())
            .collect()
    } else {
        config
            .enabled_builtin_patterns
            .split(',')
            .map(ToOwned::to_owned)
            .collect()
    };

    for name in pattern_names {
        if let Some(pattern) = builtin_patterns().get(name.as_str()) {
            config.patterns.insert(name, (*pattern).to_string());
        }
    }

    config.alphabet = alphabet_map()[config.keyboard_layout.as_str()]
        .chars()
        .filter(|ch| !DISALLOWED_CHARS.contains(ch))
        .map(|ch| ch.to_string())
        .collect();

    Ok(config)
}

pub fn validate_options(option_names: &[String], tmux: &Tmux) -> Result<(), String> {
    let mut errors = Vec::new();
    for option in option_names {
        let option_method = option_to_method(option);
        if !Config::members().contains(&option_method.as_str())
            && !option_method.starts_with("pattern_")
            && !PRIVATE_OPTIONS.contains(&option_method.as_str())
        {
            errors.push(format!("'{}' is not a valid option", option));
            tmux.exec(&format!("set-option -ug {}", option))
                .map_err(|err| err.to_string())?;
        }
    }

    if errors.is_empty() {
        return Ok(());
    }

    let mut msg = String::from("[tmux-fingers-rs] Errors found in tmux.conf:\n");
    for error in errors {
        msg.push_str("  - ");
        msg.push_str(&error);
        msg.push('\n');
    }
    Err(msg.trim_end().to_string())
}

pub fn setup_bindings(tmux: &Tmux, config: &Config) -> Result<(), String> {
    let cli = current_exe_string()?;
    setup_bindings_with_cli(tmux, config, &cli)
}

fn setup_bindings_with_cli(tmux: &Tmux, config: &Config, cli: &str) -> Result<(), String> {
    if config.enable_bindings {
        setup_root_bindings(tmux, config, cli)?;
    }
    setup_fingers_mode_bindings(tmux, cli)?;
    tmux.exec(&format!(
        "set-option -g @fingers-cli {}",
        shell_words::quote(cli)
    ))?;
    Ok(())
}

fn setup_root_bindings(tmux: &Tmux, config: &Config, cli: &str) -> Result<(), String> {
    let log_path = crate::fingers::dirs::log_path().display().to_string();
    let start_command = format!(
        "{} start \"#{{pane_id}}\" >>{} 2>&1",
        shell_words::quote(cli),
        shell_words::quote(&log_path)
    );
    tmux.exec(&format!(
        "bind-key {} run-shell -b {}",
        shell_words::quote(&config.key),
        shell_words::quote(&start_command)
    ))?;
    let jump_command = format!(
        "{} start --mode jump \"#{{pane_id}}\" >>{} 2>&1",
        shell_words::quote(cli),
        shell_words::quote(&log_path)
    );
    tmux.exec(&format!(
        "bind-key {} run-shell -b {}",
        shell_words::quote(&config.jump_key),
        shell_words::quote(&jump_command)
    ))?;
    Ok(())
}

fn setup_fingers_mode_bindings(tmux: &Tmux, cli: &str) -> Result<(), String> {
    for char_code in b'a'..=b'z' {
        let ch = char::from(char_code);
        if DISALLOWED_CHARS.contains(&ch) {
            continue;
        }
        fingers_mode_bind(tmux, cli, &ch.to_string(), &format!("hint:{}:main", ch))?;
        fingers_mode_bind(
            tmux,
            cli,
            &ch.to_uppercase().to_string(),
            &format!("hint:{}:shift", ch),
        )?;
        fingers_mode_bind(
            tmux,
            cli,
            &format!("C-{}", ch),
            &format!("hint:{}:ctrl", ch),
        )?;
        fingers_mode_bind(tmux, cli, &format!("M-{}", ch), &format!("hint:{}:alt", ch))?;
    }

    for (key, command) in [
        ("Space", "fzf"),
        ("C-c", "exit"),
        ("q", "exit"),
        ("Escape", "exit"),
        ("?", "toggle-help"),
        ("Enter", "noop"),
        ("Tab", "toggle-multi-mode"),
        ("Any", "noop"),
    ] {
        fingers_mode_bind(tmux, cli, key, command)?;
    }
    Ok(())
}

fn fingers_mode_bind(tmux: &Tmux, cli: &str, key: &str, command: &str) -> Result<(), String> {
    let input_command = format!(
        "{} send-input {}",
        shell_words::quote(cli),
        shell_words::quote(command)
    );
    tmux.exec(&format!(
        "bind-key -Tfingers {} run-shell -b {}",
        shell_words::quote(key),
        shell_words::quote(&input_command)
    ))?;
    Ok(())
}

fn current_exe_string() -> Result<String, String> {
    std::env::current_exe()
        .map_err(|err| err.to_string())
        .map(|path| path.to_string_lossy().into_owned())
}

fn shell_safe_options(
    tmux: &Tmux,
    option_names: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    for option in option_names {
        options.insert(option_to_method(option), tmux.show_option(option)?);
    }
    Ok(options)
}

fn option_to_method(option: &str) -> String {
    option.trim_start_matches("@fingers-").replace('-', "_")
}

fn check_pattern(pattern: &str) -> Result<(), String> {
    Regex::new(pattern).map(|_| ()).map_err(|err| {
        format!("[tmux-fingers-rs] Invalid pattern: {pattern}\n[tmux-fingers-rs] {err}")
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{fingers::config::Config, tmux::Tmux};

    use super::{
        option_to_method, parse_options, setup_bindings, setup_bindings_with_cli, validate_options,
    };

    #[test]
    fn converts_tmux_option_names() {
        assert_eq!(option_to_method("@fingers-main-action"), "main_action");
    }

    #[test]
    fn parses_options_into_config() {
        let tmux = Tmux::fake("3.3a");
        let options = BTreeMap::from([
            ("key".to_string(), "F".to_string()),
            ("jump_key".to_string(), "J".to_string()),
            ("keyboard_layout".to_string(), "qwerty".to_string()),
            ("main_action".to_string(), ":copy:".to_string()),
            (
                "enabled_builtin_patterns".to_string(),
                "ip,diff".to_string(),
            ),
            ("pattern_0".to_string(), "foo(?<match>bar)".to_string()),
        ]);

        let config = parse_options(options, &tmux).unwrap();
        assert!(config.patterns.contains_key("0"));
        assert!(config.patterns.contains_key("ip"));
        assert!(config.patterns.contains_key("diff"));
        assert!(
            !config
                .alphabet
                .iter()
                .any(|ch| ["c", "i", "m", "q", "n"].contains(&ch.as_str()))
        );
    }

    #[test]
    fn invalid_options_are_reported_and_unset() {
        let tmux = Tmux::fake("3.3a");
        let error = validate_options(&["@fingers-nope".to_string()], &tmux).unwrap_err();
        assert!(error.contains("'@fingers-nope' is not a valid option"));
        assert!(
            tmux.executed_commands()
                .iter()
                .any(|cmd| cmd == "set-option -ug @fingers-nope")
        );
    }

    #[test]
    fn setup_bindings_emits_root_and_mode_binds() {
        let tmux = Tmux::fake("3.3a");
        let config = Config {
            alphabet: vec!["a".into()],
            ..Config::default()
        };
        setup_bindings(&tmux, &config).unwrap();
        let executed = tmux.executed_commands();
        assert!(
            executed
                .iter()
                .any(|cmd| cmd.contains("bind-key F run-shell -b"))
        );
        assert!(
            executed
                .iter()
                .any(|cmd| cmd.contains("bind-key J run-shell -b"))
        );
        assert!(executed.iter().any(|cmd| {
            cmd.contains("bind-key -Tfingers") && cmd.contains("send-input hint:a:main")
        }));
        assert!(
            executed
                .iter()
                .any(|cmd| cmd.contains("set-option -g @fingers-cli"))
        );
    }

    #[test]
    fn setup_bindings_quotes_cli_paths_with_spaces() {
        let tmux = Tmux::fake("3.3a");
        let config = Config::default();
        let cli = "/tmp/tmux fingers/bin/tmux-fingers";

        setup_bindings_with_cli(&tmux, &config, cli).unwrap();

        let executed = tmux.executed_commands();
        let quoted_cli = shell_words::quote(cli);
        assert!(executed.iter().any(|cmd| {
            cmd.contains("bind-key F run-shell -b")
                && cmd.contains("/tmp/tmux fingers/bin/tmux-fingers")
                && cmd.contains("start")
        }));
        assert!(
            executed
                .iter()
                .any(|cmd| cmd.contains(&format!("set-option -g @fingers-cli {quoted_cli}")))
        );
    }
}
