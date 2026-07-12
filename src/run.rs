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

use std::{collections::HashMap, io::Write, path::PathBuf, process::Command};

use serde::Deserialize;

use crate::environment::CixEnvironment;

#[derive(Debug, Deserialize)]
struct CixWorkflow {
    name: Option<String>,
    jobs: HashMap<String, CixJobContainer>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CixJobContainer {
    Single(CixJob),
    Multiple(Vec<CixJob>),
}

#[derive(Debug, Deserialize)]
struct CixJob {
    name: Option<String>,
    #[serde(rename = "buildSystem")]
    build_system: String,
    #[serde(rename = "hostSystem")]
    host_system: String,
    strategy: Option<CixStrategy>,
    needs: Option<Vec<String>>,
    steps: Vec<CixStep>,
}

#[derive(Debug, Deserialize)]
struct CixStrategy {
    #[serde(rename = "fail-fast")]
    fail_fast: bool,
}

#[derive(Debug, Deserialize)]
struct CixStep {
    run: PathBuf,
    teardown: Option<PathBuf>,
}

pub(crate) fn cix_run(workflow: PathBuf, environment: &CixEnvironment) -> color_eyre::Result<()> {
    let path = if workflow.is_relative() {
        format!(
            r#"./. + {}"#,
            serde_json::to_string(&serde_json::to_string(&workflow)?)?
        )
    } else {
        format!(
            r#"/. + {}"#,
            serde_json::to_string(&serde_json::to_string(&workflow)?)?
        )
    };
    let secrets_json = serde_json::to_string(&serde_json::to_string(
        &environment.secrets.keys().collect::<Vec<_>>(),
    )?)?;
    let vars_json = serde_json::to_string(&serde_json::to_string(&environment.vars)?)?;
    let command = format!(
        "(import ./nix/workflow.nix {{ }}) ({path}) {{ secrets = builtins.fromJSON {secrets_json}; vars = builtins.fromJSON {vars_json}; }}"
    );

    let output = Command::new("nix-instantiate")
        .args(["--eval", "--strict", "--raw", "-E"])
        .arg(command)
        .output()?;

    if !output.status.success() {
        let mut stderr = std::io::stderr();
        stderr.write_all(&output.stderr)?;
        stderr.flush()?;
        return Err(color_eyre::eyre::eyre!("Failed to evaluate cix workflow"));
    }

    let workflow: CixWorkflow = serde_json::from_slice(&output.stdout)?;
    println!("{:?}", &workflow);

    Ok(())
}
