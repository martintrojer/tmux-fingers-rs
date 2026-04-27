use std::collections::BTreeMap;
use std::process::Command;

pub trait Shell {
    fn exec(&self, cmd: &str) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct ShellExec;

impl Shell for ShellExec {
    fn exec(&self, cmd: &str) -> Result<String, String> {
        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(cmd)
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

pub struct TmuxStylePrinter<S = ShellExec> {
    shell: S,
    applied_styles: BTreeMap<String, String>,
    reset_sequence: Option<String>,
}

impl<S: Shell> TmuxStylePrinter<S> {
    pub fn new(shell: S) -> Self {
        Self {
            shell,
            applied_styles: BTreeMap::new(),
            reset_sequence: None,
        }
    }

    pub fn print(&mut self, input: &str, reset_styles_after: bool) -> Result<String, String> {
        self.applied_styles.clear();
        let mut output = String::new();

        for style in input.split([' ', ',']).filter(|s| !s.is_empty()) {
            output.push_str(&self.parse_style_definition(style)?);
        }

        if reset_styles_after && !self.applied_styles.is_empty() {
            output.push_str(&self.reset_sequence()?);
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
        let layer_cmd = match layer {
            "bg" => "setab",
            "fg" => "setaf",
            _ => return Err(format!("Invalid color definition: {style}")),
        };

        if color == "default" {
            self.applied_styles.remove(layer);
            return self.reset_to_applied_styles();
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

        let result = self.shell.exec(&format!("tput {layer_cmd} {code}"))?;
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

        let mapped = match style_name {
            "bright" | "bold" => "bold",
            "dim" => "dim",
            "underscore" => "smul",
            "reverse" => "rev",
            "italics" => "sitm",
            _ => return Err(format!("Invalid style definition: {style_name}")),
        };

        let result = if style_name == "dim" {
            "\u{001b}[2m".to_string()
        } else {
            self.shell.exec(&format!("tput {mapped}"))?
        };

        if remove {
            self.applied_styles.remove(style_name);
            return self.reset_to_applied_styles();
        }

        self.applied_styles
            .insert(style_name.to_string(), result.clone());
        Ok(result)
    }

    fn reset_to_applied_styles(&mut self) -> Result<String, String> {
        let mut result = self.reset_sequence()?;
        for value in self.applied_styles.values() {
            result.push_str(value);
        }
        Ok(result)
    }

    fn reset_sequence(&mut self) -> Result<String, String> {
        if let Some(value) = &self.reset_sequence {
            return Ok(value.clone());
        }
        let value = self.shell.exec("tput sgr0")?;
        self.reset_sequence = Some(value.clone());
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{Shell, TmuxStylePrinter};

    struct FakeShell;

    impl Shell for FakeShell {
        fn exec(&self, cmd: &str) -> Result<String, String> {
            Ok(format!("$({cmd})"))
        }
    }

    #[test]
    fn prints_tmux_styles() {
        let mut printer = TmuxStylePrinter::new(FakeShell);
        let result = printer
            .print("bg=red,fg=yellow,bold", true)
            .expect("style output");
        assert_eq!(
            result,
            "$(tput setab 1)$(tput setaf 3)$(tput bold)$(tput sgr0)"
        );
    }
}
