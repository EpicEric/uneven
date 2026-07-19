// uneven: A Nix-based distributed command runner
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

use std::{
    io::{Write, stdout},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

use clap::{CommandFactory, Parser, ValueEnum};
use color_eyre::eyre::eyre;

use crate::environment::UnevenEnvironment;

mod builder;
mod environment;
mod job;
mod project;
mod secret;
mod step;
mod workflow;

#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum CheckoutStrategy {
    Default,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
enum Command {
    Run {
        workflow: PathBuf,
        #[arg(long)]
        env_file: Option<PathBuf>,
        #[arg(long)]
        eval: bool,
        #[arg(
            long,
            value_enum,
            default_value_t = CheckoutStrategy::Default,
            value_name = "STRATEGY",
        )]
        checkout: CheckoutStrategy,
    },
    Completions {
        shell: clap_complete::Shell,
    },
    Step {
        #[arg(long)]
        derivation: PathBuf,
        #[arg(long)]
        env: String,
    },
    Build {
        #[arg(long)]
        derivation: PathBuf,
    },
}

fn main() -> color_eyre::Result<()> {
    match Command::parse() {
        Command::Run {
            workflow,
            env_file,
            eval,
            checkout,
        } => {
            let mut environment =
                UnevenEnvironment::get_for_workflow(&workflow, env_file.as_ref())?;
            environment.run_workflow(workflow, eval, checkout)?;
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Command::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
        }
        Command::Step { derivation, env } => {
            let environment = UnevenEnvironment::get_for_step()?;
            environment.run_step(derivation, &serde_json::from_str(&env)?)?;
        }
        Command::Build { derivation } => {
            if derivation.exists() {
                let mut stdout = stdout();
                stdout.write_all(derivation.as_os_str().as_bytes())?;
                stdout.flush()?;
            } else {
                return Err(eyre!("Failed to build {}", derivation.to_string_lossy()));
            }
        }
    }
    Ok(())
}
