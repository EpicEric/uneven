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

use clap::Parser;

use crate::run::run;

mod run;
mod secrets;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct StepCommand {
    /// Which derivation to run.
    #[arg(long)]
    derivation: PathBuf,
    /// JSON-serialized list of envvars containing secrets.
    #[arg(long)]
    secrets: String,
}

fn main() -> color_eyre::Result<()> {
    let StepCommand {
        derivation,
        secrets,
    } = StepCommand::parse();
    run(derivation, &serde_json::from_str(&secrets)?)
}
