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

use std::path::PathBuf;

use clap::{CommandFactory, Parser, ValueEnum};

use crate::environment::NowEnvironment;

mod builder;
mod environment;
mod job;
mod project;
mod secret;
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
    /// Initialize a basic workflow.
    Init {
        /// Path to the workflow.
        workflow: Option<PathBuf>,
    },
    /// Run a workflow.
    Run {
        /// Path to the workflow.
        workflow: PathBuf,
        /// Jobs to target in this run. If unspecified, all jobs are run.
        #[arg(long = "job")]
        jobs: Vec<String>,
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
}

fn main() -> color_eyre::Result<()> {
    match Command::parse() {
        Command::Init { workflow } => {
            let path = workflow.unwrap_or(PathBuf::from("now.nix"));
            if path.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "'{}' already exists",
                    path.to_string_lossy(),
                ));
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, include_bytes!("init.nix"))?;
            eprintln!(
                "'{}' has been initialized with a basic workflow",
                path.to_string_lossy(),
            )
        }
        Command::Run {
            mut workflow,
            jobs,
            env_file,
            eval,
            checkout,
        } => {
            if workflow.is_dir() {
                let now = workflow.join("now.nix");
                if now.exists() && !now.is_dir() {
                    workflow = now;
                } else {
                    return Err(color_eyre::eyre::eyre!(
                        "Workflow 'now.nix' not found in directory '{}'",
                        workflow.to_string_lossy()
                    ));
                }
            } else if !workflow.exists() {
                return Err(color_eyre::eyre::eyre!(
                    "Workflow '{}' not found",
                    workflow.to_string_lossy()
                ));
            }
            let mut environment = NowEnvironment::get(&workflow, env_file.as_ref())?;
            environment.run_workflow(workflow, jobs, eval, checkout)?;
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Command::command(),
                env!("CARGO_BIN_NAME"),
                &mut std::io::stdout(),
            );
        }
    }
    Ok(())
}
