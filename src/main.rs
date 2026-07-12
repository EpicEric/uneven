// cix: A Nix-based CI helper
// Copyright (C) 2026 Eric Rodrigues Pires
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for
// more details.
//
// You should have received a copy of the GNU Affero General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use clap::{CommandFactory, Parser};

use crate::{environment::CixEnvironment, run::cix_run};

mod environment;
mod job;
mod run;
mod secret;
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
        #[arg(long)]
        teardown: bool,
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
        Command::Run { workflow } => {
            let environment = CixEnvironment::get()?;
            cix_run(workflow, &environment)?;
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Command::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
        }
        Command::Step {
            script,
            teardown,
            name,
        } => todo!(),
        Command::Build { derivation } => todo!(),
        Command::Upload { name, derivation } => todo!(),
        Command::Download { name } => todo!(),
    }
    Ok(())
}
