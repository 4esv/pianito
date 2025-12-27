//! onkey - CLI Piano Tuner
//!
//! A terminal-based piano tuning application with guided coaching.

use clap::Parser;
use onkey::config::{Args, Command};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Command::Analyze { file }) => {
            println!("Analyzing {}...", file);
            todo!("Implement analyze command")
        }
        Some(Command::Reference { note, duration }) => {
            println!("Playing {} for {}s...", note, duration);
            todo!("Implement reference command")
        }
        Some(Command::History) => {
            println!("Tuning history:");
            todo!("Implement history command")
        }
        Some(Command::Reset) => {
            println!("Clearing saved sessions...");
            todo!("Implement reset command")
        }
        None => {
            if args.resume {
                println!("Resuming previous session...");
            } else if args.quick {
                println!("Starting quick tune mode...");
            } else {
                println!("Starting concert pitch tuning (A4 = {}Hz)...", args.a4);
            }
            todo!("Implement interactive tuning")
        }
    }
}
