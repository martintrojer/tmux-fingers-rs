fn slice_chars(input: &str, start: usize, end: usize) -> String {
    input
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[derive(Debug, Clone)]
pub struct MatchFormatter {
    hint_style: String,
    highlight_style: String,
    selected_hint_style: String,
    selected_highlight_style: String,
    backdrop_style: String,
    hint_position: String,
    reset_sequence: String,
}

impl MatchFormatter {
    pub fn new(
        hint_style: impl Into<String>,
        highlight_style: impl Into<String>,
        selected_hint_style: impl Into<String>,
        selected_highlight_style: impl Into<String>,
        backdrop_style: impl Into<String>,
        hint_position: impl Into<String>,
        reset_sequence: impl Into<String>,
    ) -> Self {
        Self {
            hint_style: hint_style.into(),
            highlight_style: highlight_style.into(),
            selected_hint_style: selected_hint_style.into(),
            selected_highlight_style: selected_highlight_style.into(),
            backdrop_style: backdrop_style.into(),
            hint_position: hint_position.into(),
            reset_sequence: reset_sequence.into(),
        }
    }

    pub fn format(
        &self,
        hint: &str,
        highlight: &str,
        selected: bool,
        offset: Option<(usize, usize)>,
    ) -> String {
        format!(
            "{}{}{}{}",
            self.reset_sequence,
            self.before_offset(offset, highlight),
            self.format_offset(selected, hint, &self.within_offset(offset, highlight)),
            self.after_offset(offset, highlight)
        ) + &self.backdrop_style
    }

    fn before_offset(&self, offset: Option<(usize, usize)>, highlight: &str) -> String {
        offset
            .map(|(start, _)| {
                format!(
                    "{}{}",
                    self.backdrop_style,
                    slice_chars(highlight, 0, start)
                )
            })
            .unwrap_or_default()
    }

    fn within_offset(&self, offset: Option<(usize, usize)>, highlight: &str) -> String {
        match offset {
            Some((start, length)) => slice_chars(highlight, start, start + length),
            None => highlight.to_string(),
        }
    }

    fn after_offset(&self, offset: Option<(usize, usize)>, highlight: &str) -> String {
        offset
            .map(|(start, length)| {
                format!(
                    "{}{}",
                    self.backdrop_style,
                    slice_chars(highlight, start + length, highlight.chars().count())
                )
            })
            .unwrap_or_default()
    }

    fn format_offset(&self, selected: bool, hint: &str, highlight: &str) -> String {
        let chopped_highlight = self.chop_highlight(hint, highlight);
        let hint_pair = format!(
            "{}{}",
            if selected {
                &self.selected_hint_style
            } else {
                &self.hint_style
            },
            hint
        );
        let highlight_pair = format!(
            "{}{}",
            if selected {
                &self.selected_highlight_style
            } else {
                &self.highlight_style
            },
            chopped_highlight
        );

        if self.hint_position == "right" {
            format!(
                "{}{}{}{}",
                highlight_pair, self.reset_sequence, hint_pair, self.reset_sequence
            )
        } else {
            format!(
                "{}{}{}{}",
                hint_pair, self.reset_sequence, highlight_pair, self.reset_sequence
            )
        }
    }

    fn chop_highlight(&self, hint: &str, highlight: &str) -> String {
        let hint_len = hint.chars().count();
        let highlight_len = highlight.chars().count();
        if self.hint_position == "right" {
            slice_chars(highlight, 0, highlight_len.saturating_sub(hint_len))
        } else {
            slice_chars(highlight, hint_len, highlight_len)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MatchFormatter;

    struct SetupOptions<'a> {
        hint_style: &'a str,
        highlight_style: &'a str,
        hint_position: &'a str,
        selected_hint_style: &'a str,
        selected_highlight_style: &'a str,
        selected: bool,
        offset: Option<(usize, usize)>,
        hint: &'a str,
        highlight: &'a str,
    }

    fn setup(options: SetupOptions<'_>) -> String {
        let formatter = MatchFormatter::new(
            options.hint_style,
            options.highlight_style,
            options.selected_hint_style,
            options.selected_highlight_style,
            "#[bg=black,fg=white]",
            options.hint_position,
            "#[reset]",
        );

        formatter.format(
            options.hint,
            options.highlight,
            options.selected,
            options.offset,
        )
    }

    #[test]
    fn places_hint_on_left() {
        let result = setup(SetupOptions {
            hint_style: "#[fg=yellow,bold]",
            highlight_style: "#[fg=yellow]",
            hint_position: "left",
            selected_hint_style: "#[fg=green,bold]",
            selected_highlight_style: "#[fg=green]",
            selected: false,
            offset: None,
            hint: "a",
            highlight: "yolo",
        });
        assert_eq!(
            result,
            "#[reset]#[fg=yellow,bold]a#[reset]#[fg=yellow]olo#[reset]#[bg=black,fg=white]"
        );
    }

    #[test]
    fn places_hint_on_right() {
        let result = setup(SetupOptions {
            hint_style: "#[fg=yellow,bold]",
            highlight_style: "#[fg=yellow]",
            hint_position: "right",
            selected_hint_style: "#[fg=green,bold]",
            selected_highlight_style: "#[fg=green]",
            selected: false,
            offset: None,
            hint: "a",
            highlight: "yolo",
        });
        assert_eq!(
            result,
            "#[reset]#[fg=yellow]yol#[reset]#[fg=yellow,bold]a#[reset]#[bg=black,fg=white]"
        );
    }

    #[test]
    fn selects_correct_style() {
        let result = setup(SetupOptions {
            hint_style: "#[fg=yellow,bold]",
            highlight_style: "#[fg=yellow]",
            hint_position: "left",
            selected_hint_style: "#[fg=green,bold]",
            selected_highlight_style: "#[fg=green]",
            selected: true,
            offset: None,
            hint: "a",
            highlight: "yolo",
        });
        assert_eq!(
            result,
            "#[reset]#[fg=green,bold]a#[reset]#[fg=green]olo#[reset]#[bg=black,fg=white]"
        );
    }

    #[test]
    fn only_highlights_offset() {
        let result = setup(SetupOptions {
            hint_style: "#[fg=yellow,bold]",
            highlight_style: "#[fg=yellow]",
            hint_position: "left",
            selected_hint_style: "#[fg=green,bold]",
            selected_highlight_style: "#[fg=green]",
            selected: false,
            offset: Some((1, 5)),
            hint: "a",
            highlight: "yoloyoloyolo",
        });
        assert_eq!(
            result,
            "#[reset]#[bg=black,fg=white]y#[fg=yellow,bold]a#[reset]#[fg=yellow]loyo#[reset]#[bg=black,fg=white]loyolo#[bg=black,fg=white]"
        );
    }
}
