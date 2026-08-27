use std::collections::BTreeMap;

use pcre2::bytes::Regex;

use crate::fingers::config::{Config, alphabet_map, builtin_patterns};
use crate::tmux::Tmux;

const PRIVATE_OPTIONS: &[&str] = &["skip_wizard", "cli"];
const DISALLOWED_CHARS: &[char] = &['c', 'i', 'm', 'q', 'n'];
const HINT_POSITIONS: &[&str] = &["left", "right"];
const BUILTIN_ACTIONS: &[&str] = &[":copy:", ":open:", ":paste:"];

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
            "main_action" => config.main_action = check_action(&option, value)?,
            "ctrl_action" => config.ctrl_action = check_action(&option, value)?,
            "alt_action" => config.alt_action = check_action(&option, value)?,
            "shift_action" => config.shift_action = check_action(&option, value)?,
            "use_system_clipboard" => config.use_system_clipboard = parse_bool(&value),
            "benchmark_mode" => config.benchmark_mode = normalize_flag(&value),
            "hint_position" => config.hint_position = check_one_of(&option, value, HINT_POSITIONS)?,
            "hint_style" => config.hint_style = check_style(&option, &value, tmux)?,
            "selected_hint_style" => {
                config.selected_hint_style = check_style(&option, &value, tmux)?
            }
            "highlight_style" => config.highlight_style = check_style(&option, &value, tmux)?,
            "backdrop_style" => config.backdrop_style = check_style(&option, &value, tmux)?,
            "selected_highlight_style" => {
                config.selected_highlight_style = check_style(&option, &value, tmux)?
            }
            "show_copied_notification" => config.show_copied_notification = normalize_flag(&value),
            "enabled_builtin_patterns" => config.enabled_builtin_patterns = value,
            "enable_bindings" => config.enable_bindings = parse_bool(&value),
            _ => {}
        }
    }

    for (name, pattern) in user_defined_patterns {
        config.patterns.insert(name, pattern);
    }

    let builtins = builtin_patterns();
    let pattern_names: Vec<String> = if config.enabled_builtin_patterns == "all" {
        builtins.keys().map(|name| (*name).to_string()).collect()
    } else if config.enabled_builtin_patterns.is_empty() {
        // Deviation from upstream: upstream's MultiEnumParser rejects "", so it
        // offers no way to disable every builtin. We treat it as "none".
        Vec::new()
    } else {
        config
            .enabled_builtin_patterns
            .split(',')
            // Deviation from upstream: upstream does not trim, so "ip, diff"
            // is a config error there. Trimming only widens what we accept.
            .map(str::trim)
            .map(ToOwned::to_owned)
            .collect()
    };

    for name in pattern_names {
        if let Some(pattern) = builtins.get(name.as_str()) {
            config.patterns.insert(name, (*pattern).to_string());
        } else if name != "all" {
            // Upstream validates every token against ["all", *BUILTIN_PATTERNS.keys],
            // then silently skips "all" when it appears inside a longer list.
            return Err(invalid_value(
                "enabled_builtin_patterns",
                &name,
                &format!(
                    "expected \"all\" or a comma separated list of: {}",
                    builtins.keys().copied().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
    }

    let alphabet = alphabet_map()
        .get(config.keyboard_layout.as_str())
        .copied()
        .ok_or_else(|| {
            invalid_value(
                "keyboard_layout",
                &config.keyboard_layout,
                &format!(
                    "expected one of: {}",
                    alphabet_map()
                        .keys()
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
    config.alphabet = alphabet
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

fn option_display_name(option: &str) -> String {
    format!("@fingers-{}", option.replace('_', "-"))
}

fn invalid_value(option: &str, value: &str, expected: &str) -> String {
    format!(
        "[tmux-fingers-rs] Invalid value for {}: {:?}\n[tmux-fingers-rs] {expected}",
        option_display_name(option),
        value
    )
}

/// Upstream's `BoolParser` is `value == "1" || value.downcase == "true"` and
/// declares no `valid?`, so every value is accepted and anything that is not
/// truthy is simply false. Rejecting other values here would break configs
/// that work on upstream.
fn parse_bool(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

/// Flags kept as strings because the rest of the port compares them to "1";
/// normalized so that upstream's truthy spellings (`true`) still enable them.
fn normalize_flag(value: &str) -> String {
    if parse_bool(value) { "1" } else { "0" }.to_string()
}

fn check_one_of(option: &str, value: String, allowed: &[&str]) -> Result<String, String> {
    if allowed.contains(&value.as_str()) {
        return Ok(value);
    }
    Err(invalid_value(
        option,
        &value,
        &format!("expected one of: {}", allowed.join(", ")),
    ))
}

/// Actions are arbitrary shell commands, except for the `:name:` built-ins.
fn check_action(option: &str, value: String) -> Result<String, String> {
    if !(value.starts_with(':') && value.ends_with(':')) {
        return Ok(value);
    }
    check_one_of(option, value, BUILTIN_ACTIONS)
}

fn check_style(option: &str, value: &str, tmux: &Tmux) -> Result<String, String> {
    tmux.parse_style(value)
        .map_err(|err| invalid_value(option, value, &err))
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

    fn parse(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let tmux = Tmux::fake("3.3a");
        let options = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        parse_options(options, &tmux)
    }

    #[test]
    fn rejects_malformed_enum_values() {
        let err = parse(&[("hint_position", "middle")]).unwrap_err();
        assert!(err.contains("@fingers-hint-position"), "{err}");
        assert!(err.contains("expected one of: left, right"), "{err}");

        let err = parse(&[("keyboard_layout", "nope")]).unwrap_err();
        assert!(err.contains("@fingers-keyboard-layout"), "{err}");
        assert!(err.contains("qwerty"), "{err}");
    }

    #[test]
    fn parses_booleans_the_way_upstream_does() {
        // Upstream's BoolParser accepts anything: "1"/"true" are truthy, the
        // rest is false. It never reports a validation error.
        assert!(parse(&[("enable_bindings", "1")]).unwrap().enable_bindings);
        assert!(
            parse(&[("enable_bindings", "TrUe")])
                .unwrap()
                .enable_bindings
        );
        assert!(
            !parse(&[("enable_bindings", "yes")])
                .unwrap()
                .enable_bindings
        );
        assert!(
            !parse(&[("use_system_clipboard", "0")])
                .unwrap()
                .use_system_clipboard
        );

        // String-valued flags are normalized so downstream "1" comparisons
        // still see upstream's truthy spellings.
        assert_eq!(
            parse(&[("show_copied_notification", "true")])
                .unwrap()
                .show_copied_notification,
            "1"
        );
        assert_eq!(
            parse(&[("benchmark_mode", "nope")]).unwrap().benchmark_mode,
            "0"
        );
    }

    #[test]
    fn rejects_unknown_builtin_actions_but_allows_shell_commands() {
        let err = parse(&[("main_action", ":nope:")]).unwrap_err();
        assert!(err.contains("@fingers-main-action"), "{err}");
        assert!(err.contains(":copy:"), "{err}");

        let config = parse(&[("ctrl_action", "xargs -I{} open {}")]).unwrap();
        assert_eq!(config.ctrl_action, "xargs -I{} open {}");

        // A bare ":" starts and ends with ":", so upstream treats it as a
        // built-in action name and rejects it.
        assert!(parse(&[("alt_action", ":")]).is_err());

        // The empty string is the documented default for @fingers-alt-action.
        assert_eq!(parse(&[("alt_action", "")]).unwrap().alt_action, "");
    }

    #[test]
    fn rejects_malformed_styles_and_patterns() {
        let err = parse(&[("hint_style", "#[fg=notacolor]")]).unwrap_err();
        assert!(err.contains("@fingers-hint-style"), "{err}");

        let err = parse(&[("pattern_bad", "foo(")]).unwrap_err();
        assert!(err.contains("Invalid pattern"), "{err}");
    }

    #[test]
    fn validates_each_enabled_builtin_pattern_name() {
        let err = parse(&[("enabled_builtin_patterns", "ip,nope,diff")]).unwrap_err();
        assert!(err.contains("@fingers-enabled-builtin-patterns"), "{err}");
        assert!(err.contains("nope"), "{err}");

        let config = parse(&[("enabled_builtin_patterns", "ip, diff")]).unwrap();
        assert_eq!(config.patterns.len(), 2);

        // Upstream validates against ["all", *BUILTIN_PATTERNS.keys], so "all"
        // inside a longer list is accepted and then skipped by name lookup.
        let config = parse(&[("enabled_builtin_patterns", "all,ip")]).unwrap();
        assert_eq!(config.patterns.keys().collect::<Vec<_>>(), vec!["ip"]);
    }

    #[test]
    fn user_defined_patterns_are_always_enabled_but_not_valid_builtin_names() {
        // @fingers-pattern-* patterns are added unconditionally...
        let config = parse(&[
            ("pattern_mine", "foo(?<match>bar)"),
            ("enabled_builtin_patterns", "ip"),
        ])
        .unwrap();
        assert!(config.patterns.contains_key("mine"));
        assert!(config.patterns.contains_key("ip"));

        // ...but naming one in @fingers-enabled-builtin-patterns is a config
        // error, matching upstream's MultiEnumParser.
        let err = parse(&[
            ("pattern_mine", "foo(?<match>bar)"),
            ("enabled_builtin_patterns", "mine,ip"),
        ])
        .unwrap_err();
        assert!(err.contains("mine"), "{err}");
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
