// now: A Nix-based distributed command runner
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

use crate::environment::NowEnvironment;

mod builder;
mod environment;
mod job;
mod project;
mod secret;
mod step;
mod utils;
mod workflow;

#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum CheckoutStrategy {
    /// Don't checkout; create a fresh directory for every job.
    None,
    /// On the local builder, run commands at the local directory.
    /// On remote builders, copy non-ignored files from the local directory via rsync.
    Default,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
enum Command {
    /// Run a workflow.
    Run {
        /// Path to the workflow.
        workflow: PathBuf,
        /// Optional dotenv file to read environment variables from.
        #[arg(long)]
        env_file: Option<PathBuf>,
        /// Evaluate but don't run the workflow.
        #[arg(long)]
        eval: bool,
        /// Strategy for checking out the current working directory.
        #[arg(
            long,
            value_enum,
            default_value_t = CheckoutStrategy::Default,
            value_name = "STRATEGY",
        )]
        checkout: CheckoutStrategy,
    },
    /// Generate shell completions.
    Completions {
        /// Which shell to generate completions for.
        shell: clap_complete::Shell,
    },
    /// INTERNAL: Command used to run a job step.
    Step {
        /// Which derivation to run.
        #[arg(long)]
        derivation: PathBuf,
        /// JSON-serialized environment for the step.
        #[arg(long)]
        env: String,
    },
    /// INTERNAL: Command used to build a derivation.
    Build {
        /// Which derivation to build.
        #[arg(long)]
        derivation: PathBuf,
    },
}

fn main() -> color_eyre::Result<()> {
    match Command::parse() {
        Command::Run {
            mut workflow,
            env_file,
            eval,
            checkout,
        } => {
            if workflow.is_dir() {
                workflow.push("default.nix");
            }
            if !workflow.exists() {
                return Err(eyre!("Workflow '{}' not found", workflow.to_string_lossy()));
            }
            let mut environment = NowEnvironment::get_for_workflow(&workflow, env_file.as_ref())?;
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
            let environment = NowEnvironment::get_for_step()?;
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
