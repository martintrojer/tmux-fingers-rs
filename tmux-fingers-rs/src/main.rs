mod cli;
mod fingers;
mod huffman;
mod priority_queue;
mod tmux;
mod tmux_style_printer;

use clap::Parser;

fn main() -> std::process::ExitCode {
    match cli::Cli::parse().run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            std::process::ExitCode::FAILURE
        }
    }
}
