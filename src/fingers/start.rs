use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

use crate::fingers::action_runner::{ActionRunner, PaneInfo};
use crate::fingers::config::Config;
use crate::fingers::dirs;
use crate::fingers::hinter::{Hinter, HinterOptions, Printer, Target};
use crate::fingers::input_socket::InputSocket;
use crate::fingers::state::State;
use crate::tmux::{Pane, Tmux};

const CLEAR_SEQ: &str = "\u{1b}[H\u{1b}[J";
const HIDE_CURSOR_SEQ: &str = "\u{1b}[?25l";

#[derive(Debug, Clone)]
pub struct StartOptions {
    pub pane_id: String,
    pub mode: String,
    pub patterns: Option<String>,
    pub main_action: Option<String>,
    pub ctrl_action: Option<String>,
    pub alt_action: Option<String>,
    pub shift_action: Option<String>,
}

pub fn run_start(tmux: &Tmux, config: &Config, options: StartOptions) -> Result<(), String> {
    let runner = StartRunner::new(tmux.clone(), config.clone(), options)?;
    runner.run()
}

struct StartRunner {
    tmux: Tmux,
    config: Config,
    options: StartOptions,
    target_pane: Pane,
    active_pane: Pane,
    patterns: Vec<String>,
}

impl StartRunner {
    fn new(tmux: Tmux, config: Config, options: StartOptions) -> Result<Self, String> {
        let (target_pane, active_pane) = parse_pane_target_format(&tmux, &options.pane_id)?;
        let patterns = patterns_from_options(&config, options.patterns.as_deref(), &tmux)?;
        Ok(Self {
            tmux,
            config,
            options,
            target_pane,
            active_pane,
            patterns,
        })
    }

    fn run(self) -> Result<(), String> {
        let track = self.track_tmux_state()?;
        let pane_contents = self
            .tmux
            .capture_pane(&self.target_pane, self.options.mode != "jump")?;
        let pane_contents = pane_contents
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let fingers_window = self.tmux.create_window("[fingers]", "cat", 80, 24)?;
        let cleanup = CleanupState {
            tmux: self.tmux.clone(),
            track,
            target_pane_id: self.target_pane.pane_id.clone(),
            active_pane_id: self.active_pane.pane_id.clone(),
            fingers_pane_id: fingers_window.pane_id.clone(),
        };
        let mut printer = PanePrinter::new(&fingers_window.pane_tty)?;
        let state = State::default();

        let render = |printer: &mut PanePrinter, state: &State| {
            let context = RenderContext {
                config: &self.config,
                target_pane: &self.target_pane,
                pane_contents: &pane_contents,
                patterns: &self.patterns,
                reuse_hints: self.options.mode != "jump",
            };
            render_view(printer, &context, state)
        };

        let render_context = RenderContext {
            config: &self.config,
            target_pane: &self.target_pane,
            pane_contents: &pane_contents,
            patterns: &self.patterns,
            reuse_hints: self.options.mode != "jump",
        };
        let result = (|| -> Result<(), String> {
            let targets = show_hints(
                &self.tmux,
                &fingers_window,
                &mut printer,
                &state,
                &render_context,
            )?;

            if self.config.benchmark_mode == "1" {
                return Ok(());
            }

            let (state, targets) = self.handle_input(state, targets, render, &mut printer)?;
            self.process_result(&state, &targets)?;
            Ok(())
        })();

        merge_cleanup_result(result, cleanup.run())
    }

    fn track_tmux_state(&self) -> Result<TrackedTmuxState, String> {
        let output = self.tmux.exec(
            "display-message -t '{last}' -p '#{pane_id};#{client_key_table};#{prefix};#{prefix2}'",
        )?;
        let mut parts = output.split(';');
        let last_pane_id = parts.next().unwrap_or_default().to_string();
        let last_key_table = match parts.next().unwrap_or_default() {
            "" => "root".to_string(),
            value => value.to_string(),
        };
        let prefix = parts.next().unwrap_or_default().to_string();
        let prefix2 = parts.next().unwrap_or_default().to_string();
        Ok(TrackedTmuxState {
            last_pane_id,
            last_key_table,
            prefix,
            prefix2,
        })
    }

    fn handle_input<F>(
        &self,
        mut state: State,
        mut targets: BTreeMap<String, Target>,
        render: F,
        printer: &mut PanePrinter,
    ) -> Result<(State, BTreeMap<String, Target>), String>
    where
        F: Fn(&mut PanePrinter, &State) -> Result<BTreeMap<String, Target>, String>,
    {
        let input_socket = InputSocket::new(dirs::socket_path());
        self.tmux.disable_prefix()?;
        self.tmux.set_key_table("fingers")?;

        let error = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let error_clone = error.clone();
        input_socket
            .on_input(|input| {
                match process_input(
                    &mut state,
                    &mut targets,
                    &input,
                    &render,
                    printer,
                    &self.options.mode,
                ) {
                    Ok(should_continue) => should_continue,
                    Err(err) => {
                        *error_clone.lock().unwrap() = Some(err);
                        false
                    }
                }
            })
            .map_err(|err| err.to_string())?;

        if let Some(err) = error.lock().unwrap().clone() {
            return Err(err);
        }

        Ok((state, targets))
    }

    fn process_result(
        &self,
        state: &State,
        targets: &BTreeMap<String, Target>,
    ) -> Result<(), String> {
        if state.result.is_empty() {
            return Ok(());
        }

        let offset = targets.get(&state.input).map(|target| target.offset);
        ActionRunner {
            modifier: state.modifier.clone(),
            r#match: state.result.clone(),
            hint: state.input.clone(),
            original_pane: PaneInfo {
                pane_id: self.active_pane.pane_id.clone(),
                pane_current_path: self.active_pane.pane_current_path.clone(),
                pane_in_mode: self.active_pane.pane_in_mode,
            },
            offset,
            mode: self.options.mode.clone(),
            main_action: self.options.main_action.clone(),
            ctrl_action: self.options.ctrl_action.clone(),
            alt_action: self.options.alt_action.clone(),
            shift_action: self.options.shift_action.clone(),
        }
        .run(&self.config, &self.tmux)?;

        if !state.result.is_empty() && self.config.show_copied_notification == "1" {
            self.tmux
                .display_message(&format!("Copied: {}", state.result), 1000)?;
        }

        Ok(())
    }
}

fn patterns_from_options(
    config: &Config,
    requested: Option<&str>,
    tmux: &Tmux,
) -> Result<Vec<String>, String> {
    if let Some(patterns) = requested {
        let mut result = Vec::new();
        for name in patterns.split(',') {
            if let Some(pattern) = config.patterns.get(name) {
                result.push(pattern.clone());
            } else {
                tmux.display_message(
                    &format!("[tmux-fingers-rs] error: Unknown pattern {name}"),
                    5000,
                )?;
                return Err(format!("Unknown pattern {name}"));
            }
        }
        Ok(result)
    } else {
        Ok(config.patterns.values().cloned().collect())
    }
}

fn parse_pane_target_format(tmux: &Tmux, pane_target: &str) -> Result<(Pane, Pane), String> {
    if pane_target.starts_with('%') && pane_target[1..].chars().all(|ch| ch.is_ascii_digit()) {
        let target_pane = tmux
            .find_pane_by_id(pane_target)?
            .ok_or_else(|| format!("Unknown pane {pane_target}"))?;
        return Ok((target_pane.clone(), target_pane));
    }

    let pane_id = tmux.exec(&format!(
        "display-message -t {} -p '#{{pane_id}}'",
        shell_words::quote(pane_target)
    ))?;
    let pane_id = pane_id.trim().to_string();
    let target_pane = tmux
        .find_pane_by_id(&pane_id)?
        .ok_or_else(|| format!("Unknown pane {pane_id}"))?;
    let active_pane = tmux
        .list_panes("#{pane_active}", &target_pane.window_id)?
        .into_iter()
        .next()
        .ok_or_else(|| "Missing active pane".to_string())?;
    Ok((target_pane, active_pane))
}

struct RenderContext<'a> {
    config: &'a Config,
    target_pane: &'a Pane,
    pane_contents: &'a [String],
    patterns: &'a [String],
    reuse_hints: bool,
}

fn show_hints(
    tmux: &Tmux,
    fingers_window: &crate::tmux::Window,
    printer: &mut PanePrinter,
    state: &State,
    context: &RenderContext<'_>,
) -> Result<BTreeMap<String, Target>, String> {
    if needs_resize(context.target_pane, context.pane_contents) {
        tmux.resize_window(
            &fingers_window.window_id,
            context.target_pane.pane_width,
            context.target_pane.pane_height,
        )?;
    }

    let targets = if context.target_pane.window_zoomed_flag {
        tmux.swap_panes(&fingers_window.pane_id, &context.target_pane.pane_id)?;
        render_view(printer, context, state)?
    } else {
        let targets = render_view(printer, context, state)?;
        tmux.swap_panes(&fingers_window.pane_id, &context.target_pane.pane_id)?;
        targets
    };

    Ok(targets)
}

fn render_view(
    printer: &mut PanePrinter,
    context: &RenderContext<'_>,
    state: &State,
) -> Result<BTreeMap<String, Target>, String> {
    printer.print(CLEAR_SEQ);
    printer.print(HIDE_CURSOR_SEQ);

    let mut hinter = Hinter::new(
        HinterOptions {
            input: context.pane_contents.to_vec(),
            width: context.target_pane.pane_width as usize,
            current_input: state.input.clone(),
            selected_hints: state.selected_hints.clone(),
            patterns: context.patterns.to_vec(),
            alphabet: context.config.alphabet.clone(),
            reuse_hints: context.reuse_hints,
            hint_style: context.config.hint_style.clone(),
            highlight_style: context.config.highlight_style.clone(),
            selected_hint_style: context.config.selected_hint_style.clone(),
            selected_highlight_style: context.config.selected_highlight_style.clone(),
            backdrop_style: context.config.backdrop_style.clone(),
            hint_position: context.config.hint_position.clone(),
            reset_sequence: Config::reset_sequence().to_string(),
        },
        printer,
    );
    hinter.run()?;
    Ok(hinter.targets())
}

fn process_input<F, P: Printer>(
    state: &mut State,
    targets: &mut BTreeMap<String, Target>,
    input: &str,
    render: &F,
    printer: &mut P,
    mode: &str,
) -> Result<bool, String>
where
    F: Fn(&mut P, &State) -> Result<BTreeMap<String, Target>, String>,
{
    let mut parts = input.split(':');
    let command = parts.next().unwrap_or_default();
    match command {
        "hint" => {
            let ch = parts.next().unwrap_or_default();
            let modifier = parts.next().unwrap_or_default();
            state.input.push_str(ch);
            state.modifier = modifier.to_string();

            if let Some(target) = targets.get(&state.input) {
                if state.multi_mode {
                    state.multi_matches.push(target.text.clone());
                    state.selected_hints.push(state.input.clone());
                    state.input.clear();
                    *targets = render(printer, state)?;
                } else {
                    state.result = target.text.clone();
                    state.exiting = true;
                }
            } else {
                *targets = render(printer, state)?;
            }
        }
        "exit" => state.exiting = true,
        "toggle-multi-mode" if mode != "jump" => {
            let was_multi = state.multi_mode;
            state.multi_mode = !state.multi_mode;
            if was_multi && !state.multi_mode {
                state.result = state.multi_matches.join(" ");
                state.exiting = true;
            }
        }
        "toggle-help" | "fzf" | "noop" | "" => {}
        _ => {}
    }

    Ok(!state.exiting)
}

fn needs_resize(target_pane: &Pane, pane_contents: &[String]) -> bool {
    let pane_width = target_pane.pane_width as usize;
    pane_contents.iter().any(|line| {
        let char_len = line.chars().count();
        line.len() > char_len || char_len > pane_width
    })
}

struct PanePrinter {
    file: File,
}

impl PanePrinter {
    fn new(path: &str) -> Result<Self, String> {
        let file = File::options()
            .write(true)
            .open(path)
            .map_err(|err| err.to_string())?;
        Ok(Self { file })
    }
}

impl Printer for PanePrinter {
    fn print(&mut self, msg: &str) {
        let _ = self.file.write_all(msg.as_bytes());
    }

    fn flush(&mut self) {
        let _ = self.file.flush();
    }
}

struct TrackedTmuxState {
    last_pane_id: String,
    last_key_table: String,
    prefix: String,
    prefix2: String,
}

struct CleanupState {
    tmux: Tmux,
    track: TrackedTmuxState,
    target_pane_id: String,
    active_pane_id: String,
    fingers_pane_id: String,
}

impl CleanupState {
    fn run(self) -> Result<(), String> {
        let mut errors = Vec::new();

        if let Err(err) = self
            .tmux
            .swap_panes(&self.fingers_pane_id, &self.target_pane_id)
        {
            errors.push(err);
        }
        if let Err(err) = self.tmux.kill_pane(&self.fingers_pane_id) {
            errors.push(err);
        }
        if let Err(err) = self.tmux.select_pane(&self.track.last_pane_id) {
            errors.push(err);
        }
        if let Err(err) = self.tmux.select_pane(&self.active_pane_id) {
            errors.push(err);
        }
        if let Err(err) = self.tmux.set_key_table(&self.track.last_key_table) {
            errors.push(err);
        }
        if let Err(err) = self.tmux.set_global_option("prefix", &self.track.prefix) {
            errors.push(err);
        }
        if let Err(err) = self.tmux.set_global_option("prefix2", &self.track.prefix2) {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn merge_cleanup_result(
    result: Result<(), String>,
    cleanup: Result<(), String>,
) -> Result<(), String> {
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(err)) => Err(err),
        (Err(err), Ok(())) => Err(err),
        (Err(err), Err(cleanup_err)) => Err(format!("{err}; cleanup failed: {cleanup_err}")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::fingers::config::{Config, builtin_patterns};

    use super::{
        CleanupState, State, Target, TrackedTmuxState, needs_resize, patterns_from_options,
        process_input,
    };

    struct NullPrinter;
    impl super::Printer for NullPrinter {
        fn print(&mut self, _msg: &str) {}
        fn flush(&mut self) {}
    }

    #[test]
    fn selects_requested_patterns() {
        let config = Config {
            patterns: builtin_patterns()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Config::default()
        };
        let tmux = crate::tmux::Tmux::fake("3.3a");
        let patterns = patterns_from_options(&config, Some("ip,diff"), &tmux).unwrap();
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn hint_input_selects_result() {
        let mut state = State::default();
        let mut targets = BTreeMap::from([(
            "a".to_string(),
            Target {
                text: "match".into(),
                hint: "a".into(),
                offset: (0, 0),
            },
        )]);
        let mut printer = NullPrinter;
        let expected_targets = targets.clone();
        let render = move |_printer: &mut NullPrinter, _state: &State| Ok(expected_targets.clone());

        let should_continue = process_input(
            &mut state,
            &mut targets,
            "hint:a:main",
            &render,
            &mut printer,
            "default",
        )
        .unwrap();
        assert!(!should_continue);
        assert_eq!(state.result, "match");
        assert_eq!(state.modifier, "main");
    }

    #[test]
    fn resize_detects_wrapped_or_wide_lines() {
        let pane = crate::tmux::Pane {
            pane_id: "%1".into(),
            window_id: "@1".into(),
            pane_width: 3,
            pane_height: 5,
            pane_current_path: "/tmp".into(),
            pane_in_mode: false,
            scroll_position: None,
            window_zoomed_flag: false,
        };
        assert!(needs_resize(&pane, &["abcd".into()]));
        assert!(needs_resize(&pane, &["中".into()]));
    }

    #[test]
    fn parses_explicit_pane_ids() {
        let pane_response = "%1;@1;80;24;/tmp;false;;false".to_string();
        let tmux = crate::tmux::Tmux::fake_with_responses(
            "3.3a",
            [(
                "display-message -t '%1' -F '#{pane_id};#{window_id};#{pane_width};#{pane_height};#{pane_current_path};#{?pane_in_mode,true,false};#{?scroll_position,#{scroll_position},};#{?window_zoomed_flag,true,false}' -p".to_string(),
                pane_response,
            )],
        );

        let (target, active) = super::parse_pane_target_format(&tmux, "%1").unwrap();
        assert_eq!(target.pane_id, "%1");
        assert_eq!(active.pane_id, "%1");
    }

    #[test]
    fn parses_target_formats_via_tmux_lookup() {
        let responses = [
            (
                "display-message -t {last} -p '#{pane_id}'".to_string(),
                "%2".to_string(),
            ),
            (
                "display-message -t '%2' -F '#{pane_id};#{window_id};#{pane_width};#{pane_height};#{pane_current_path};#{?pane_in_mode,true,false};#{?scroll_position,#{scroll_position},};#{?window_zoomed_flag,true,false}' -p".to_string(),
                "%2;@9;90;30;/tmp;false;;false".to_string(),
            ),
            (
                "list-panes -F '#{pane_id};#{window_id};#{pane_width};#{pane_height};#{pane_current_path};#{?pane_in_mode,true,false};#{?scroll_position,#{scroll_position},};#{?window_zoomed_flag,true,false}' -t @9 -f '#{pane_active}'".to_string(),
                "%3;@9;90;30;/work;false;;false".to_string(),
            ),
        ];
        let tmux = crate::tmux::Tmux::fake_with_responses("3.3a", responses);

        let (target, active) = super::parse_pane_target_format(&tmux, "{last}").unwrap();
        assert_eq!(target.pane_id, "%2");
        assert_eq!(active.pane_id, "%3");
    }

    #[test]
    fn cleanup_restores_tmux_state() {
        let tmux = crate::tmux::Tmux::fake("3.3a");
        let cleanup = CleanupState {
            tmux: tmux.clone(),
            track: TrackedTmuxState {
                last_pane_id: "%9".into(),
                last_key_table: "root".into(),
                prefix: "C-b".into(),
                prefix2: "None".into(),
            },
            target_pane_id: "%1".into(),
            active_pane_id: "%2".into(),
            fingers_pane_id: "%3".into(),
        };

        cleanup.run().unwrap();

        let executed = tmux.executed_commands();
        assert!(
            executed
                .iter()
                .any(|cmd| cmd == "swap-pane -d -s '%3' -t '%1' -Z")
        );
        assert!(executed.iter().any(|cmd| cmd == "kill-pane -t '%3'"));
        assert!(
            executed
                .iter()
                .any(|cmd| cmd == "set-window-option key-table root")
        );
        assert!(executed.iter().any(|cmd| cmd == "set-option -g prefix C-b"));
    }
}
