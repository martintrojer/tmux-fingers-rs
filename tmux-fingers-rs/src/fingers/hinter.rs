use std::collections::{BTreeMap, BTreeSet};

use pcre2::bytes::Regex;

use crate::fingers::match_formatter::MatchFormatter;
use crate::huffman::Huffman;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub text: String,
    pub hint: String,
    pub offset: (usize, usize),
}

pub trait Printer {
    fn print(&mut self, msg: &str);
    fn flush(&mut self);
}

pub struct Hinter<'a, P: Printer> {
    lines: Vec<String>,
    width: usize,
    current_input: String,
    selected_hints: Vec<String>,
    output: &'a mut P,
    formatter: MatchFormatter,
    patterns: Vec<String>,
    alphabet: Vec<String>,
    reuse_hints: bool,
    target_by_hint: BTreeMap<String, Target>,
    target_by_text: BTreeMap<String, Target>,
}

pub struct HinterOptions {
    pub input: Vec<String>,
    pub width: usize,
    pub current_input: String,
    pub selected_hints: Vec<String>,
    pub patterns: Vec<String>,
    pub alphabet: Vec<String>,
    pub reuse_hints: bool,
    pub hint_style: String,
    pub highlight_style: String,
    pub selected_hint_style: String,
    pub selected_highlight_style: String,
    pub backdrop_style: String,
    pub hint_position: String,
    pub reset_sequence: String,
}

impl<'a, P: Printer> Hinter<'a, P> {
    pub fn new(options: HinterOptions, output: &'a mut P) -> Self {
        Self {
            lines: options.input,
            width: options.width,
            current_input: options.current_input,
            selected_hints: options.selected_hints,
            output,
            formatter: MatchFormatter::new(
                options.hint_style,
                options.highlight_style,
                options.selected_hint_style,
                options.selected_highlight_style,
                options.backdrop_style,
                options.hint_position,
                options.reset_sequence,
            ),
            patterns: options.patterns,
            alphabet: options.alphabet,
            reuse_hints: options.reuse_hints,
            target_by_hint: BTreeMap::new(),
            target_by_text: BTreeMap::new(),
        }
    }

    pub fn run(&mut self) -> Result<(), String> {
        let hints = Huffman.generate_hints(&self.alphabet, self.n_matches()?);
        let pattern = compile_pattern(&self.patterns)?;
        let mut hint_index = hints.len();
        self.target_by_hint.clear();
        self.target_by_text.clear();

        for line_index in 0..self.lines.len() {
            let line = self.lines[line_index].clone();
            let line_out =
                self.process_line(line_index, &line, &pattern, &hints, &mut hint_index)?;
            self.output.print(&line_out);
            if line_index + 1 != self.lines.len() {
                self.output.print("\n");
            }
        }
        self.output.flush();
        let _ = self.width;
        Ok(())
    }

    pub fn targets(&self) -> BTreeMap<String, Target> {
        self.target_by_hint.clone()
    }

    fn process_line(
        &mut self,
        line_index: usize,
        line: &str,
        pattern: &Regex,
        hints: &[String],
        hint_index: &mut usize,
    ) -> Result<String, String> {
        let mut result = String::new();
        let mut last = 0usize;
        let bytes = line.as_bytes();

        for captures in pattern.captures_iter(bytes) {
            let captures = captures.map_err(|err| err.to_string())?;
            let full = captures.get(0).ok_or_else(|| "missing match".to_string())?;
            let full_start = full.start();
            let full_end = full.end();

            result.push_str(&line[last..full_start]);
            let full_text =
                std::str::from_utf8(&bytes[full_start..full_end]).map_err(|err| err.to_string())?;

            let capture = captures
                .name("match")
                .or_else(|| captures.get(0))
                .ok_or_else(|| "missing capture".to_string())?;
            let capture_start = capture.start();
            let capture_end = capture.end();
            let captured_text = std::str::from_utf8(&bytes[capture_start..capture_end])
                .map_err(|err| err.to_string())?;

            let relative_start =
                line[..capture_start].chars().count() - line[..full_start].chars().count();
            let capture_len = captured_text.chars().count();
            let absolute_offset = (line_index, line[..capture_start].chars().count());

            let hint = if self.reuse_hints {
                if let Some(existing) = self.target_by_text.get(captured_text) {
                    existing.hint.clone()
                } else {
                    *hint_index = hint_index.saturating_sub(1);
                    hints
                        .get(*hint_index)
                        .cloned()
                        .ok_or_else(|| "Too many matches".to_string())?
                }
            } else {
                *hint_index = hint_index.saturating_sub(1);
                hints
                    .get(*hint_index)
                    .cloned()
                    .ok_or_else(|| "Too many matches".to_string())?
            };

            if hint.chars().count() > captured_text.chars().count() {
                result.push_str(full_text);
                last = full_end;
                continue;
            }

            let target = Target {
                text: captured_text.to_string(),
                hint: hint.clone(),
                offset: absolute_offset,
            };
            self.target_by_hint.insert(hint.clone(), target.clone());
            self.target_by_text
                .insert(captured_text.to_string(), target.clone());

            if !self.current_input.is_empty() && !hint.starts_with(&self.current_input) {
                result.push_str(full_text);
            } else {
                result.push_str(&self.formatter.format(
                    &hint,
                    full_text,
                    self.selected_hints.contains(&hint),
                    if capture_start == full_start && capture_end == full_end {
                        None
                    } else {
                        Some((relative_start, capture_len))
                    },
                ));
            }
            last = full_end;
        }

        result.push_str(&line[last..]);
        Ok(result)
    }

    fn n_matches(&self) -> Result<usize, String> {
        let pattern = compile_pattern(&self.patterns)?;

        if self.reuse_hints {
            let mut set = BTreeSet::new();
            for line in &self.lines {
                for captures in pattern.captures_iter(line.as_bytes()) {
                    let captures = captures.map_err(|err| err.to_string())?;
                    let capture = captures
                        .name("match")
                        .or_else(|| captures.get(0))
                        .ok_or_else(|| "missing capture".to_string())?;
                    set.insert(line[capture.start()..capture.end()].to_string());
                }
            }
            Ok(set.len())
        } else {
            let mut count = 0usize;
            for line in &self.lines {
                for captures in pattern.captures_iter(line.as_bytes()) {
                    captures.map_err(|err| err.to_string())?;
                    count += 1;
                }
            }
            Ok(count)
        }
    }
}

fn compile_pattern(patterns: &[String]) -> Result<Regex, String> {
    Regex::new(&format!("(?J)({})", patterns.join("|"))).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Hinter, HinterOptions, Printer};
    use crate::fingers::config::builtin_patterns;

    #[derive(Default)]
    struct TextOutput {
        contents: String,
    }

    impl Printer for TextOutput {
        fn print(&mut self, msg: &str) {
            self.contents.push_str(msg);
        }

        fn flush(&mut self) {}
    }

    fn generate_lines() -> String {
        let mut input = String::new();
        for row in 0..50 {
            if row > 0 {
                input.push('\n');
            }
            for col in 0..10 {
                if col > 0 {
                    input.push(' ');
                }
                input.push_str(&format!("{:016}", row * 10 + col));
            }
        }
        input
    }

    fn test_options(
        input: Vec<String>,
        patterns: Vec<String>,
        alphabet: Vec<String>,
        reuse_hints: bool,
    ) -> HinterOptions {
        HinterOptions {
            input,
            width: 100,
            current_input: String::new(),
            selected_hints: Vec::new(),
            patterns,
            alphabet,
            reuse_hints,
            hint_style: "<hint>".into(),
            highlight_style: "<highlight>".into(),
            selected_hint_style: "<selected-hint>".into(),
            selected_highlight_style: "<selected-highlight>".into(),
            backdrop_style: "<backdrop>".into(),
            hint_position: "left".into(),
            reset_sequence: "<reset>".into(),
        }
    }

    #[test]
    fn works_in_grid_of_lines() {
        let input = generate_lines();
        let mut output = TextOutput::default();
        let patterns = builtin_patterns()
            .values()
            .map(|pattern| pattern.to_string())
            .collect::<Vec<_>>();
        let alphabet = vec!["a", "s", "d", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        let mut hinter = Hinter::new(
            test_options(
                input.lines().map(ToOwned::to_owned).collect(),
                patterns,
                alphabet,
                false,
            ),
            &mut output,
        );

        hinter.run().unwrap();
        assert!(!output.contents.is_empty());
    }

    #[test]
    fn highlights_captured_groups() {
        let input = r#"
On branch ruby-rewrite-more-like-crystal-rewrite-amirite
Your branch is up to date with 'origin/ruby-rewrite-more-like-crystal-rewrite-amirite'.

Changes to be committed:
        modified:   spec/lib/fingers/match_formatter_spec.cr
"#;
        let mut output = TextOutput::default();
        let mut patterns = builtin_patterns()
            .values()
            .map(|pattern| pattern.to_string())
            .collect::<Vec<_>>();
        patterns.push("On branch (?<match>.*)".to_string());
        let alphabet = vec!["a", "s", "d", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        let mut hinter = Hinter::new(
            test_options(
                input.lines().map(ToOwned::to_owned).collect(),
                patterns,
                alphabet,
                false,
            ),
            &mut output,
        );

        hinter.run().unwrap();
        assert!(
            output
                .contents
                .contains("ruby-rewrite-more-like-crystal-rewrite-amirite")
        );
    }

    #[test]
    fn can_rerender_without_reusing_hints() {
        let input = r#"
        modified:   src/fingers/cli.cr
        modified:   src/fingers/cli.cr
        modified:   src/fingers/cli.cr
"#;
        let mut output = TextOutput::default();
        let patterns = builtin_patterns()
            .values()
            .map(|pattern| pattern.to_string())
            .collect::<Vec<_>>();
        let alphabet = vec!["a", "s", "d", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let mut hinter = Hinter::new(
            test_options(
                input.lines().map(ToOwned::to_owned).collect(),
                patterns,
                alphabet,
                false,
            ),
            &mut output,
        );

        hinter.run().unwrap();
        hinter.run().unwrap();
    }
}
