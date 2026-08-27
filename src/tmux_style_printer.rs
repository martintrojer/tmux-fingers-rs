use std::collections::BTreeMap;

use crate::fingers::config::ANSI_RESET;

#[derive(Debug, Default)]
pub struct TmuxStylePrinter {
    applied_styles: BTreeMap<String, String>,
}

impl TmuxStylePrinter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn print(&mut self, input: &str, reset_styles_after: bool) -> Result<String, String> {
        self.applied_styles.clear();
        let mut output = String::new();

        for style in input.split([' ', ',']).filter(|s| !s.is_empty()) {
            output.push_str(&self.parse_style_definition(style)?);
        }

        if reset_styles_after && !self.applied_styles.is_empty() {
            output.push_str(ANSI_RESET);
        }

        Ok(output)
    }

    fn parse_style_definition(&mut self, style: &str) -> Result<String, String> {
        if style.starts_with("bg=") || style.starts_with("fg=") {
            self.parse_color(style)
        } else {
            self.parse_style(style)
        }
    }

    fn parse_color(&mut self, style: &str) -> Result<String, String> {
        let (layer, color) = style
            .split_once('=')
            .ok_or_else(|| format!("Invalid color definition: {style}"))?;
        let layer_code = match layer {
            "bg" => 48,
            "fg" => 38,
            _ => return Err(format!("Invalid color definition: {style}")),
        };

        if color == "default" {
            self.applied_styles.remove(layer);
            return Ok(self.reset_to_applied_styles());
        }

        let code = if let Some(rest) = color.strip_prefix("colour") {
            rest.parse::<u8>()
                .map_err(|_| format!("Invalid color definition: {style}"))?
        } else if let Some(rest) = color.strip_prefix("color") {
            rest.parse::<u8>()
                .map_err(|_| format!("Invalid color definition: {style}"))?
        } else {
            match color {
                "black" => 0,
                "red" => 1,
                "green" => 2,
                "yellow" => 3,
                "blue" => 4,
                "magenta" => 5,
                "cyan" => 6,
                "white" => 7,
                _ => return Err(format!("Invalid color definition: {style}")),
            }
        };

        let result = format!("\u{1b}[{layer_code};5;{code}m");
        self.applied_styles
            .insert(layer.to_string(), result.clone());
        Ok(result)
    }

    fn parse_style(&mut self, style: &str) -> Result<String, String> {
        let (remove, style_name) = if let Some(stripped) = style.strip_prefix("no") {
            (true, stripped)
        } else {
            (false, style)
        };

        let result = match style_name {
            "bright" | "bold" => "\u{1b}[1m",
            "dim" => "\u{1b}[2m",
            "underscore" => "\u{1b}[4m",
            "reverse" => "\u{1b}[7m",
            "italics" => "\u{1b}[3m",
            _ => return Err(format!("Invalid style definition: {style_name}")),
        };

        if remove {
            self.applied_styles.remove(style_name);
            return Ok(self.reset_to_applied_styles());
        }

        self.applied_styles
            .insert(style_name.to_string(), result.to_string());
        Ok(result.to_string())
    }

    fn reset_to_applied_styles(&self) -> String {
        let mut result = ANSI_RESET.to_string();
        for value in self.applied_styles.values() {
            result.push_str(value);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::TmuxStylePrinter;

    #[test]
    fn transforms_tmux_style_format_into_escape_sequences() {
        let mut printer = TmuxStylePrinter::new();
        let result = printer
            .print("bg=red,fg=yellow,bold", true)
            .expect("style output");
        assert_eq!(result, "\u{1b}[48;5;1m\u{1b}[38;5;3m\u{1b}[1m\u{1b}[0m");
    }

    #[test]
    fn resets_to_remaining_styles_when_a_style_is_removed() {
        let mut printer = TmuxStylePrinter::new();
        let result = printer
            .print("fg=yellow,bold,nobold", false)
            .expect("style output");
        assert_eq!(result, "\u{1b}[38;5;3m\u{1b}[1m\u{1b}[0m\u{1b}[38;5;3m");
    }

    #[test]
    fn does_not_emit_a_reset_when_no_styles_were_applied() {
        let mut printer = TmuxStylePrinter::new();
        assert_eq!(printer.print("", true).expect("style output"), "");
    }
}
