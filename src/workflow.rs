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

use std::{collections::HashMap, io::Write, path::PathBuf, process::Command};

use serde::Deserialize;

use crate::{CheckoutStrategy, environment::UnevenEnvironment, project::create_project_source};

#[derive(Debug, Deserialize)]
pub(crate) struct UnevenWorkflow {
    pub(crate) name: Option<String>,
    pub(crate) jobs: HashMap<String, UnevenJobContainer>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum UnevenJobContainer {
    Single(UnevenJob),
    Multiple(Vec<UnevenJob>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnevenJob {
    pub(crate) name: Option<String>,
    #[serde(rename = "buildSystem")]
    pub(crate) build_system: String,
    #[serde(rename = "hostSystem")]
    pub(crate) host_system: String,
    #[serde(rename = "system-features")]
    pub(crate) system_features: Vec<String>,
    pub(crate) strategy: Option<UnevenStrategy>,
    pub(crate) needs: Option<Vec<String>>,
    pub(crate) steps: Vec<UnevenStep>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnevenStrategy {
    #[serde(rename = "fail-fast")]
    pub(crate) fail_fast: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnevenStep {
    pub(crate) run: PathBuf,
    pub(crate) teardown: Option<PathBuf>,
    pub(crate) env: HashMap<String, UnevenStepEnvVar>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum UnevenStepEnvVar {
    Plain(String),
    Secret(UnevenStepSecret),
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnevenStepSecret {
    #[serde(rename = "__unevenSecret")]
    pub(crate) secret_name: String,
}

impl UnevenEnvironment {
    pub(crate) fn run_workflow(
        &mut self,
        workflow: PathBuf,
        dry_run: bool,
        show_trace: bool,
        checkout: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        let workflow_canonical = std::fs::canonicalize(&workflow)?;
        let workflow_str = workflow_canonical
            .to_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("non-UTF8 path"))?;
        let workflow_path = format!("(/. + {})", serde_json::to_string(&workflow_str)?);

        let mut nix_workflow = create_project_source()?;
        nix_workflow.push("nix");
        nix_workflow.push("workflow.nix");
        let nix_workflow_canonical = std::fs::canonicalize(&nix_workflow)?;
        let nix_workflow_str = nix_workflow_canonical
            .to_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("non-UTF8 path"))?;
        let nix_workflow_path = format!("(/. + {})", serde_json::to_string(&nix_workflow_str)?);

        let secrets_json = serde_json::to_string(&serde_json::to_string(
            &self.secrets.keys().collect::<Vec<_>>(),
        )?)?;
        let vars_json = serde_json::to_string(&serde_json::to_string(&self.vars)?)?;

        let nix_command = format!(
            "(import {nix_workflow_path} {{ }}) {workflow_path} {{ secrets = builtins.fromJSON {secrets_json}; vars = builtins.fromJSON {vars_json}; }}"
        );

        let mut command = Command::new("nix-instantiate");
        command.args(["--impure", "--eval", "--strict", "--raw"]);
        if show_trace {
            command.arg("--show-trace");
        }
        let output = command.arg("--expr").arg(nix_command).output()?;

        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to evaluate uneven workflow"
            ));
        }

        let workflow: UnevenWorkflow = serde_json::from_slice(&output.stdout)?;

        if dry_run {
            println!("{:?}", &workflow);
            return Ok(());
        }

        Ok(())
    }
}
