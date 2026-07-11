use std::path::PathBuf;

use clap::{CommandFactory, Parser};

mod job;
mod run;
mod step;

#[derive(Parser)]
#[command(version, about, long_about = None)]
enum Command {
    Run {
        workflow: PathBuf,
    },
    Completions {
        shell: clap_complete::Shell,
    },
    Step {
        #[arg(long)]
        script: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Build {
        #[arg(long)]
        derivation: PathBuf,
    },
    Upload {
        #[arg(long)]
        name: String,
        #[arg(long)]
        derivation: PathBuf,
    },
    Download {
        #[arg(long)]
        name: String,
    },
}

fn main() -> color_eyre::Result<()> {
    match Command::parse() {
        Command::Run { workflow } => todo!(),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Command::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
            return Ok(());
        }
        Command::Step { script, name } => todo!(),
        Command::Build { derivation } => todo!(),
        Command::Upload { name, derivation } => todo!(),
        Command::Download { name } => todo!(),
    }
}
