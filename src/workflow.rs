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
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    process::Command,
};

use futures::stream::FuturesUnordered;
use owo_colors::OwoColorize;
use petgraph::{
    acyclic::Acyclic, algo::Cycle, matrix_graph::NodeIndex, stable_graph::StableDiGraph,
};
use serde::Deserialize;
use smol::{channel, stream::StreamExt};

use crate::{
    CheckoutStrategy,
    builder::{NowBuilder, local::LocalBuilder},
    environment::NowEnvironment,
    job::JobResult,
    project::create_nix_project_source,
};

#[derive(Debug, Deserialize)]
pub(crate) struct NowWorkflow {
    pub(crate) name: Option<String>,
    pub(crate) jobs: HashMap<String, NowJobContainer>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum NowJobContainer {
    Single(NowJob),
    Multiple(Vec<NowJob>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct NowJob {
    pub(crate) name: String,
    #[serde(rename = "buildSystem")]
    pub(crate) build_system: String,
    #[serde(rename = "hostSystem")]
    pub(crate) _host_system: String,
    #[serde(rename = "requiredSystemFeatures")]
    pub(crate) required_system_features: HashSet<String>,
    pub(crate) strategy: Option<NowStrategy>,
    pub(crate) needs: Option<Vec<String>>,
    pub(crate) steps: Vec<NowStep>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NowStrategy {
    #[serde(rename = "fail-fast")]
    pub(crate) fail_fast: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NowStep {
    pub(crate) name: String,
    #[serde(rename = "runDrv", default)]
    pub(crate) run_drv: PathBuf,
    #[serde(rename = "teardownDrv", default)]
    pub(crate) teardown_drv: Option<PathBuf>,
    pub(crate) env: HashMap<String, NowStepEnvVar>,
    #[serde(rename = "__nowUploadKey", default)]
    pub(crate) upload_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum NowStepEnvVar {
    Plain(String),
    Secret(NowStepSecret),
    Download(NowStepDownload),
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NowStepSecret {
    #[serde(rename = "__nowSecret")]
    pub(crate) secret_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NowStepDownload {
    #[serde(rename = "__nowDownload")]
    pub(crate) download_name: String,
}

pub(crate) struct NowWorkflowParams {
    pub(crate) workflow: PathBuf,
    pub(crate) jobs: Vec<String>,
    pub(crate) eval: bool,
    pub(crate) checkout_strategy: CheckoutStrategy,
    pub(crate) builders: Option<String>,
}

impl NowEnvironment {
    pub(crate) fn run_workflow(
        &mut self,
        NowWorkflowParams {
            workflow: workflow_path,
            jobs,
            eval,
            checkout_strategy: strategy,
            builders,
        }: NowWorkflowParams,
    ) -> color_eyre::Result<()> {
        let builder = LocalBuilder::new(self, strategy, builders)?;
        let style = builder.get_style();

        eprintln!(
            "{} Evaluating workflow '{}'...",
            format!("{}>", builder.get_name()).style(style),
            workflow_path.to_string_lossy()
        );
        let workflow = self.evaluate_workflow(&workflow_path)?;
        if eval {
            println!("{:?}", &workflow);
            return Ok(());
        }

        if let Some(name) = workflow.name.as_ref() {
            eprintln!(
                "{} Building tree for '{}'...",
                format!("{}>", builder.get_name()).style(style),
                name
            );
        } else {
            eprintln!(
                "{} Building tree for workflow...",
                format!("{}>", builder.get_name()).style(style)
            );
        }
        let NowWorkflowGraph {
            dag: mut tree,
            mut nodes,
        } = workflow.build_graph(jobs)?;

        let executor = smol::LocalExecutor::new();

        let (sender, ctrl_c) = channel::bounded(1);
        ctrlc::set_handler(move || {
            let _ = sender.try_send(());
        })?;
        let builder_ref = &builder;
        let ctrl_c_task = executor.spawn(async move {
            if ctrl_c.recv().await.is_ok() {
                builder_ref.cancel_builders();
            }
            smol::future::pending::<color_eyre::Result<()>>().await
        });

        let workflow_task = executor.spawn(async {
            let mut futures = FuturesUnordered::<Pin<Box<dyn Future<Output = JobResult>>>>::new();
            let mut result = Ok(());

            loop {
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

                for node_index in current_nodes {
                    let node_weight = &tree[node_index];
                    match node_weight {
                        DagNode::Root => {
                            debug_assert!(tree.node_count() == 0);
                        }
                        DagNode::Job => match nodes.remove(&node_index) {
                            Some(NowJobContainer::Single(job)) => {
                                futures.push(self.run_job_single(&builder, job, node_index))
                            }
                            Some(NowJobContainer::Multiple(job_vec)) => {
                                futures
                                    .push(self.run_jobs_multiple(&builder, job_vec, node_index)?);
                            }
                            None => (),
                        },
                    }
                }

                if let Some(future) = futures.next().await {
                    match future {
                        Ok(node_index) => {
                            tree.remove_node(node_index);
                        }
                        Err(error) => {
                            result = result.and(Err(error));
                        }
                    }
                } else {
                    return result;
                };
            }
        });

        smol::future::block_on(executor.run(smol::future::or(workflow_task, ctrl_c_task)))
    }

    fn evaluate_workflow(&self, workflow: &Path) -> color_eyre::Result<NowWorkflow> {
        let workflow_canonical = std::fs::canonicalize(workflow)?;
        let workflow_str = workflow_canonical
            .to_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("non-UTF8 path"))?;
        let workflow_path = format!("(/. + {})", serde_json::to_string(&workflow_str)?);

        let nix_workflow = create_nix_project_source()?.join("nix/workflow.nix");
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
            return Err(color_eyre::eyre::eyre!("Failed to evaluate workflow"));
        }

        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

#[derive(Debug)]
enum DagNode {
    Root,
    Job,
}

struct NowWorkflowGraph {
    dag: Acyclic<StableDiGraph<DagNode, ()>>,
    nodes: HashMap<NodeIndex<u32>, NowJobContainer>,
}

impl NowWorkflow {
    fn build_graph(self, jobs: Vec<String>) -> color_eyre::Result<NowWorkflowGraph> {
        let mut graph = StableDiGraph::new();
        let root = graph.add_node(DagNode::Root);

        let mut nodes: HashMap<NodeIndex<u32>, NowJobContainer> = HashMap::new();
        let mut graph_nodes: HashMap<String, NodeIndex<u32>> = HashMap::new();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();

        for (job_id, job) in self.jobs.into_iter() {
            match job {
                NowJobContainer::Single(job) => {
                    for need in job.needs.iter().flatten() {
                        edges
                            .entry(job_id.clone())
                            .or_default()
                            .insert(need.clone());
                    }
                    let node = graph.add_node(DagNode::Job);
                    nodes.insert(node, NowJobContainer::Single(job));
                    graph_nodes.insert(job_id, node);
                    graph.add_edge(node, root, ());
                }
                NowJobContainer::Multiple(job_vec) => {
                    for need in job_vec.iter().flat_map(|job| job.needs.iter().flatten()) {
                        edges
                            .entry(job_id.clone())
                            .or_default()
                            .insert(need.clone());
                    }
                    let node = graph.add_node(DagNode::Job);
                    nodes.insert(node, NowJobContainer::Multiple(job_vec));
                    graph_nodes.insert(job_id, node);
                    graph.add_edge(node, root, ());
                }
            }
        }

        for (from, to) in edges {
            for edge in to {
                graph.add_edge(
                    *graph_nodes
                        .get(&edge)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Unknown node {}", edge))?,
                    *graph_nodes
                        .get(&from)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Unknown node {}", from))?,
                    (),
                );
            }
        }

        // Prune non-target jobs
        if !jobs.is_empty() {
            let job_nodes = jobs
                .iter()
                .map(|job_id| {
                    graph_nodes
                        .get(job_id)
                        .map(|index| *index)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Unknown job '{job_id}'"))
                })
                .collect::<color_eyre::Result<HashSet<NodeIndex<u32>>>>()?;

            // Collect the set of nodes to keep
            let mut keep: HashSet<NodeIndex<u32>> = HashSet::new();
            let mut stack: Vec<NodeIndex<u32>> = job_nodes.iter().copied().collect();
            while let Some(node) = stack.pop() {
                if !keep.insert(node) {
                    continue;
                }
                for dep in graph.neighbors_directed(node, petgraph::Direction::Incoming) {
                    if dep != root && !keep.contains(&dep) {
                        stack.push(dep);
                    }
                }
            }

            graph.retain_nodes(|_, node| node == root || keep.contains(&node));
        }

        let dag = graph.try_into().map_err(|cycle: Cycle<_>| {
            color_eyre::eyre::eyre!(
                "Cycle detected on '{}'",
                graph_nodes
                    .iter()
                    .find(|(_, value)| **value == cycle.node_id())
                    .map(|(key, _)| key.clone())
                    .unwrap_or("unknown".into())
            )
        })?;

        Ok(NowWorkflowGraph { dag, nodes })
    }
}
