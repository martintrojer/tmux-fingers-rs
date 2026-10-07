use std::collections::{BTreeMap, BTreeSet};

use pcre2::bytes::{Regex, RegexBuilder};

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
    backdrop_style: String,
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
                options.backdrop_style.clone(),
                options.hint_position,
                options.reset_sequence,
            ),
            backdrop_style: options.backdrop_style,
            patterns: options.patterns,
            alphabet: options.alphabet,
            reuse_hints: options.reuse_hints,
            target_by_hint: BTreeMap::new(),
            target_by_text: BTreeMap::new(),
        }
    }

    pub fn run(&mut self) -> Result<(), String> {
        let pattern = compile_pattern(&self.patterns)?;
        let hints = Huffman.generate_hints(&self.alphabet, self.n_matches(&pattern)?);
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
        let tab_positions = tab_positions_for(line);
        let mut result = String::new();
        let mut last = 0usize;
        let mut counted_to = 0usize;
        let mut counted_chars = 0usize;
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

            counted_chars += line[counted_to..capture_start].chars().count();
            let absolute_offset = (line_index, counted_chars);
            counted_chars += line[capture_start..full_end].chars().count();
            counted_to = full_end;

            let relative_start = line[full_start..capture_start].chars().count();
            let capture_len = captured_text.chars().count();

            let (hint, popped) = if self.reuse_hints {
                if let Some(existing) = self.target_by_text.get(captured_text) {
                    (existing.hint.clone(), false)
                } else {
                    (pop_hint(hints, hint_index)?, true)
                }
            } else {
                (pop_hint(hints, hint_index)?, true)
            };

            if hint.chars().count() > capture_len {
                if popped {
                    *hint_index += 1;
                }
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
        let initial_length = result.chars().count();
        let result = expand_tabs(&result, &tab_positions);
        let tab_correction = result.chars().count().saturating_sub(initial_length);
        let double_width_correction =
            ((line.len().saturating_sub(line.chars().count())) as f64 / 3.0).round() as usize;
        let padding = self
            .width
            .saturating_sub(line.chars().count())
            .saturating_sub(double_width_correction)
            .saturating_sub(tab_correction);

        Ok(format!(
            "{}{}{}",
            self.backdrop_style,
            result,
            " ".repeat(padding)
        ))
    }

    fn n_matches(&self, pattern: &Regex) -> Result<usize, String> {
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

fn pop_hint(hints: &[String], hint_index: &mut usize) -> Result<String, String> {
    let index = hint_index
        .checked_sub(1)
        .ok_or_else(|| "Too many matches".to_string())?;
    *hint_index = index;
    hints
        .get(index)
        .cloned()
        .ok_or_else(|| "Too many matches".to_string())
}

fn tab_positions_for(line: &str) -> Vec<usize> {
    line.chars()
        .enumerate()
        .filter_map(|(index, ch)| (ch == '\t').then_some(index))
        .collect()
}

fn expand_tabs(line: &str, tab_positions: &[usize]) -> String {
    let mut positions = tab_positions.iter();
    let mut correction = 0usize;
    let mut result = String::new();

    for ch in line.chars() {
        if ch == '\t' {
            if let Some(position) = positions.next() {
                let spaces = 8 - ((position + correction) % 8);
                result.push_str(&" ".repeat(spaces));
                correction += spaces - 1;
                continue;
            }
        }
        result.push(ch);
    }

    result
}

pub fn compile_pattern(patterns: &[String]) -> Result<Regex, String> {
    RegexBuilder::new()
        .utf(true)
        .ucp(true)
        .build(&format!("(?J)({})", patterns.join("|")))
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{Hinter, HinterOptions, Printer, Target, pop_hint};
    use crate::fingers::config::{Config, builtin_patterns};

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
    fn default_config_generates_hints_for_multiple_matches() {
        let config = Config::default();
        let mut output = TextOutput::default();
        let mut hinter = Hinter::new(
            test_options(
                vec!["12345 67890".into()],
                vec![r"\d+".into()],
                config.alphabet,
                false,
            ),
            &mut output,
        );

        hinter.run().unwrap();
        assert_eq!(hinter.targets().len(), 2);
        assert!(output.contents.contains("<hint>"), "{}", output.contents);
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

    fn render(options: HinterOptions) -> (String, BTreeMap<String, Target>) {
        let mut output = TextOutput::default();
        let targets = {
            let mut hinter = Hinter::new(options, &mut output);
            hinter.run().unwrap();
            hinter.targets()
        };
        (output.contents, targets)
    }

    fn target(text: &str, hint: &str, column: usize) -> Target {
        target_at(text, hint, 0, column)
    }

    fn target_at(text: &str, hint: &str, row: usize, column: usize) -> Target {
        Target {
            text: text.into(),
            hint: hint.into(),
            offset: (row, column),
        }
    }

    #[test]
    fn unicode_matching_uses_codepoints_and_ucp() {
        let mut options = test_options(
            vec!["café 12345".into()],
            vec![r"\w+".into()],
            vec!["a".into(), "s".into()],
            false,
        );
        options.width = 10;

        let (_, targets) = render(options);

        assert_eq!(
            targets,
            BTreeMap::from([
                ("a".into(), target("12345", "a", 5)),
                ("s".into(), target("café", "s", 0)),
            ])
        );
    }

    #[test]
    fn reuse_hints_handles_multibyte_matches() {
        let mut options = test_options(
            vec!["aé1234 a❯".into()],
            vec!["a.".into()],
            vec!["a".into(), "s".into()],
            true,
        );
        options.width = 10;

        let (_, targets) = render(options);

        assert_eq!(
            targets,
            BTreeMap::from([
                ("a".into(), target("a❯", "a", 7)),
                ("s".into(), target("aé", "s", 0)),
            ])
        );
    }

    #[test]
    fn prefixes_expands_and_pads_every_line_like_upstream() {
        let mut options = test_options(
            vec!["❯\tX".into(), "é".into()],
            vec!["(?!)".into()],
            vec!["a".into(), "s".into()],
            false,
        );
        options.width = 12;

        let (output, targets) = render(options);

        assert_eq!(output, "<backdrop>❯       X  \n<backdrop>é           ");
        assert!(targets.is_empty());
    }

    #[test]
    fn puts_too_long_hints_back_for_the_next_match() {
        let mut options = test_options(
            vec!["x abcd abce abcf abcg".into()],
            vec![r"\w+".into()],
            vec!["a".into(), "s".into()],
            true,
        );
        options.width = 21;

        let (output, targets) = render(options);

        assert_eq!(
            output,
            "<backdrop>x \
             <reset><reset><hint>ssas<reset><highlight><reset><backdrop> \
             <reset><reset><hint>ssaa<reset><highlight><reset><backdrop> \
             <reset><reset><hint>sss<reset><highlight>f<reset><backdrop> \
             <reset><reset><hint>sa<reset><highlight>cg<reset><backdrop>"
        );
        assert_eq!(
            targets,
            BTreeMap::from([
                ("sa".into(), target("abcg", "sa", 17)),
                ("ssaa".into(), target("abce", "ssaa", 7)),
                ("ssas".into(), target("abcd", "ssas", 2)),
                ("sss".into(), target("abcf", "sss", 12)),
            ])
        );
        assert_eq!(
            targets
                .values()
                .map(|target| target.hint.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            targets.len()
        );
    }

    #[test]
    fn empty_hint_stack_is_an_error() {
        assert_eq!(pop_hint(&[], &mut 0), Err("Too many matches".into()));
    }

    #[test]
    fn duplicate_text_reuses_one_hint() {
        let mut options = test_options(
            vec!["same same other".into()],
            vec![r"\w+".into()],
            vec!["a".into(), "s".into()],
            true,
        );
        options.width = 15;

        let (output, targets) = render(options);

        assert_eq!(
            output,
            "<backdrop><reset><reset><hint>s<reset><highlight>ame<reset><backdrop> \
             <reset><reset><hint>s<reset><highlight>ame<reset><backdrop> \
             <reset><reset><hint>a<reset><highlight>ther<reset><backdrop>"
        );
        assert_eq!(
            targets,
            BTreeMap::from([
                ("a".into(), target("other", "a", 10)),
                ("s".into(), target("same", "s", 5)),
            ])
        );
    }

    #[test]
    fn current_input_only_formats_matching_prefixes() {
        let mut options = test_options(
            vec!["first second".into()],
            vec![r"\w+".into()],
            vec!["a".into(), "s".into()],
            false,
        );
        options.width = 12;
        options.current_input = "a".into();

        let (output, targets) = render(options);

        assert_eq!(
            output,
            "<backdrop>first <reset><reset><hint>a<reset><highlight>econd<reset><backdrop>"
        );
        assert_eq!(
            targets,
            BTreeMap::from([
                ("a".into(), target("second", "a", 6)),
                ("s".into(), target("first", "s", 0)),
            ])
        );
    }

    #[test]
    fn can_rerender_without_reusing_hints() {
        let mut output = TextOutput::default();
        let mut options = test_options(
            vec!["one".into(), "one".into(), "one".into()],
            vec![r"\w+".into()],
            vec!["a".into(), "s".into(), "d".into(), "f".into()],
            false,
        );
        options.width = 3;
        let mut hinter = Hinter::new(options, &mut output);

        hinter.run().unwrap();
        let first_targets = hinter.targets();
        hinter.run().unwrap();
        assert_eq!(hinter.targets(), first_targets);
        drop(hinter);

        let expected = "<backdrop><reset><reset><hint>f<reset><highlight>ne<reset><backdrop>\
                        \n<backdrop><reset><reset><hint>d<reset><highlight>ne<reset><backdrop>\
                        \n<backdrop><reset><reset><hint>s<reset><highlight>ne<reset><backdrop>";
        assert_eq!(output.contents, expected.repeat(2));
        assert_eq!(
            first_targets,
            BTreeMap::from([
                ("d".into(), target_at("one", "d", 1, 0)),
                ("f".into(), target("one", "f", 0)),
                ("s".into(), target_at("one", "s", 2, 0)),
            ])
        );
    }
}
