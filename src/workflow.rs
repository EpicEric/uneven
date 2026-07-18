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
    collections::{HashMap, HashSet},
    io::Write,
    path::PathBuf,
    process::Command,
};

use petgraph::{acyclic::Acyclic, algo::Cycle, graph::DiGraph, matrix_graph::NodeIndex};
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
    pub(crate) name: String,
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
    pub(crate) name: String,
    #[serde(rename = "runDrv", default)]
    pub(crate) run_drv: PathBuf,
    #[serde(rename = "teardownDrv", default)]
    pub(crate) teardown_drv: Option<PathBuf>,
    pub(crate) env: HashMap<String, UnevenStepEnvVar>,
    #[serde(rename = "__unevenUploadKey", default)]
    pub(crate) upload_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum UnevenStepEnvVar {
    Plain(String),
    Secret(UnevenStepSecret),
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UnevenStepSecret {
    #[serde(rename = "__unevenSecret")]
    pub(crate) secret_name: String,
}

impl UnevenEnvironment {
    pub(crate) fn run_workflow(
        &mut self,
        workflow_path: PathBuf,
        eval: bool,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        eprintln!(
            "Evaluating workflow '{}'...",
            workflow_path.to_string_lossy()
        );
        let workflow = self.evaluate_workflow(workflow_path)?;
        if eval {
            println!("{:?}", &workflow);
            return Ok(());
        }

        if let Some(name) = workflow.name.as_ref() {
            eprintln!("Building tree for workflow '{name}'...");
        } else {
            eprintln!("Building tree for workflow...");
        }
        let mut tree = workflow.build_graph()?;

        'tree: loop {
            let mut current_nodes: HashSet<NodeIndex<u32>> = HashSet::new();
            for node in tree.nodes_iter() {
                if tree
                    .edges_directed(node, petgraph::Direction::Incoming)
                    .next()
                    .is_none()
                {
                    current_nodes.insert(node);
                }
            }

            debug_assert!(!current_nodes.is_empty());
            for node in current_nodes {
                let node = tree.remove_node(node).expect("node exists");
                match node {
                    UnevenJobNode::Root => {
                        debug_assert!(tree.node_count() == 0);
                        break 'tree;
                    }
                    UnevenJobNode::Single(job) => {
                        self.run_job_local(job, strategy)?;
                    }
                    UnevenJobNode::Multiple(job_vec) => {
                        self.run_jobs_remote(job_vec, strategy)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn evaluate_workflow(&self, workflow: PathBuf) -> color_eyre::Result<UnevenWorkflow> {
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

        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "eval",
            "--impure",
            "--json",
            "--keep-derivations",
        ]);
        let output = command.arg("--expr").arg(nix_command).output()?;

        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to evaluate uneven workflow"
            ));
        }

        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

enum UnevenJobNode {
    Root,
    Single(UnevenJob),
    Multiple(Vec<UnevenJob>),
}

impl UnevenWorkflow {
    fn build_graph(self) -> color_eyre::Result<Acyclic<DiGraph<UnevenJobNode, ()>>> {
        let mut graph = DiGraph::new();
        let root = graph.add_node(UnevenJobNode::Root);

        let mut nodes: HashMap<String, NodeIndex<u32>> = HashMap::new();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();

        for (job_id, job) in self.jobs.into_iter() {
            match job {
                UnevenJobContainer::Single(job) => {
                    for need in job.needs.iter().flatten() {
                        edges
                            .entry(job_id.clone())
                            .or_default()
                            .insert(need.clone());
                    }
                    let node = graph.add_node(UnevenJobNode::Single(job));
                    nodes.insert(job_id, node);
                    graph.add_edge(node, root, ());
                }
                UnevenJobContainer::Multiple(job_vec) => {
                    for need in job_vec.iter().flat_map(|job| job.needs.iter().flatten()) {
                        edges
                            .entry(job_id.clone())
                            .or_default()
                            .insert(need.clone());
                    }
                    let node = graph.add_node(UnevenJobNode::Multiple(job_vec));
                    nodes.insert(job_id, node);
                    graph.add_edge(node, root, ());
                }
            }
        }

        for (from, to) in edges {
            for edge in to {
                graph.add_edge(
                    *nodes
                        .get(&edge)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Unknown node {}", edge))?,
                    *nodes
                        .get(&from)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Unknown node {}", from))?,
                    (),
                );
            }
        }

        Ok(graph.try_into().map_err(|cycle: Cycle<_>| {
            color_eyre::eyre::eyre!(
                "Cycle detected on '{}'",
                nodes
                    .iter()
                    .find(|(_, value)| **value == cycle.node_id())
                    .map(|(key, _)| key.clone())
                    .unwrap_or("unknown".into())
            )
        })?)
    }
}
