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

use std::path::PathBuf;

use clap::{CommandFactory, Parser, ValueEnum};

use crate::environment::UnevenEnvironment;

mod environment;
mod project;
mod secret;
mod step;
mod workflow;

#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum CheckoutStrategy {
    Git,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
enum Command {
    Run {
        workflow: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        show_trace: bool,
        #[arg(
            long,
            value_enum,
            default_value_t = CheckoutStrategy::Git,
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
        Command::Run {
            workflow,
            dry_run,
            show_trace,
            checkout,
        } => {
            let mut environment = UnevenEnvironment::get()?;
            environment.run_workflow(workflow, dry_run, show_trace, checkout)?;
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
            derivation,
            env,
            teardown,
            name,
        } => {
            let environment = UnevenEnvironment::get()?;
            environment.run_step(derivation, teardown, &serde_json::from_str(&env)?)?;
        }
        Command::Build { derivation } => todo!(),
        Command::Upload { name, derivation } => {
            todo!()
        }
        Command::Download { name } => {
            todo!()
        }
    }
    Ok(())
}
