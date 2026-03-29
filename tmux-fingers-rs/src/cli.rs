use std::io::{self, Write};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::fingers::{
    config::Config, dirs, input_socket::InputSocket, load_config::run_load_config, start,
};
use crate::tmux::Tmux;

#[derive(Debug, Parser)]
#[command(
    name = "tmux-fingers",
    bin_name = "tmux-fingers",
    version,
    about = "description"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Duh.")]
    Version,
    #[command(about = "Show environment and installation info")]
    Info,
    #[command(about = "Load tmux config and install bindings")]
    LoadConfig,
    #[command(hide = true)]
    SendInput(SendInputArgs),
    #[command(about = "Start tmux-fingers on a pane")]
    Start(StartArgs),
}

#[derive(Debug, Args)]
struct SendInputArgs {
    input: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Default,
    Jump,
}

#[derive(Debug, Args)]
struct StartArgs {
    #[arg(help = "pane id (also accepts tmux target-pane tokens specified in tmux man pages)")]
    pane_id: String,
    #[arg(long, value_enum, default_value_t = Mode::Default, help = "can be \"jump\" or \"default\"")]
    mode: Mode,
    #[arg(long, help = "comma separated list of pattern names")]
    patterns: Option<String>,
    #[arg(
        long = "main-action",
        help = "command to which the output will be piped"
    )]
    main_action: Option<String>,
    #[arg(
        long = "ctrl-action",
        help = "command to which the output will be piped when holding CTRL key"
    )]
    ctrl_action: Option<String>,
    #[arg(
        long = "alt-action",
        help = "command to which the output will be piped when holding ALT key"
    )]
    alt_action: Option<String>,
    #[arg(
        long = "shift-action",
        help = "command to which the output will be pipedwhen holding SHIFT key"
    )]
    shift_action: Option<String>,
}

impl Cli {
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self.command {
            Some(Command::Version) => {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
            Some(Command::Info) => {
                let mut out = io::BufWriter::new(io::stdout().lock());
                writeln!(out, "tmux-fingers-rs\t{}", env!("CARGO_PKG_VERSION"))?;
                writeln!(out, "xdg-root-folder\t{}", dirs::root_dir().display())?;
                writeln!(out, "log-path\t{}", dirs::log_path().display())?;
                writeln!(
                    out,
                    "installation-method\t{}",
                    option_env!("WIZARD_INSTALLATION_METHOD").unwrap_or("manual")
                )?;
                writeln!(
                    out,
                    "tmux-version\t{}",
                    Tmux::new()
                        .version_string()
                        .unwrap_or_else(|_| "not found".into())
                )?;
                writeln!(
                    out,
                    "TERM\t{}",
                    std::env::var("TERM").unwrap_or_else(|_| "not set".into())
                )?;
                writeln!(
                    out,
                    "SHELL\t{}",
                    std::env::var("SHELL").unwrap_or_else(|_| "not set".into())
                )?;
                writeln!(out, "rust-version\tunknown")?;
            }
            Some(Command::LoadConfig) => {
                dirs::ensure_folders()?;
                run_load_config(&Tmux::new()).map_err(io::Error::other)?;
            }
            Some(Command::SendInput(args)) => {
                let socket = InputSocket::new(dirs::socket_path());
                socket.send_message(&args.input)?;
            }
            Some(Command::Start(args)) => {
                let config = Config::load().unwrap_or_default();
                let mode = match args.mode {
                    Mode::Default => "default",
                    Mode::Jump => "jump",
                };
                start::run_start(
                    &Tmux::new(),
                    &config,
                    start::StartOptions {
                        pane_id: args.pane_id,
                        mode: mode.to_string(),
                        patterns: args.patterns,
                        main_action: args.main_action,
                        ctrl_action: args.ctrl_action,
                        alt_action: args.alt_action,
                        shift_action: args.shift_action,
                    },
                )
                .map_err(io::Error::other)?;
            }
            None => {
                let mut cmd = Cli::command();
                cmd.print_help()?;
                println!();
            }
        }

        Ok(())
    }
}
