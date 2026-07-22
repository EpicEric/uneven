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
    env::temp_dir,
    ffi::{OsStr, OsString},
    io::{PipeReader, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use owo_colors::Style;
use smol::{
    channel,
    io::AsyncReadExt,
    lock::{Mutex, futures::Lock},
    process::{Child, Command, Stdio},
};

use crate::{
    CheckoutStrategy,
    builder::{CheckoutTask, CommandCheckoutTask, NixConfig, NowBuilder, remote::RemoteBuilder},
    environment::NowEnvironment,
    utils::pipe_outputs_to_stderr,
    workflow::NowJob,
};

pub(crate) struct LocalBuilder {
    pub(crate) cancellation: channel::Sender<()>,
    pub(crate) cancellation_rx: Mutex<channel::Receiver<()>>,
    pub(crate) env: HashMap<OsString, OsString>,
    pub(crate) strategy: CheckoutStrategy,
    pub(crate) hostname: String,
    pub(crate) system: String,
    pub(crate) system_features: HashSet<String>,
    pub(crate) remote_builders: Vec<RemoteBuilder>,
}

impl LocalBuilder {
    pub(crate) fn new(
        environment: &NowEnvironment,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<Self> {
        let output = std::process::Command::new("nix")
            .args([
                "--extra-experimental-features",
                "nix-command",
                "config",
                "show",
                "--json",
            ])
            .output()?;

        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!("Failed to fetch Nix config"));
        }

        let config: NixConfig = serde_json::from_slice(&output.stdout)?;

        let remote_builders = RemoteBuilder::get_remote_builders(&config, strategy)?;

        let (cancellation, cancellation_rx) = channel::bounded(1);

        Ok(Self {
            cancellation,
            cancellation_rx: Mutex::new(cancellation_rx),
            env: environment.local_env.clone(),
            strategy,
            hostname: sys_info::hostname()?,
            system: config.system.value,
            system_features: config.system_features.value.into_iter().collect(),
            remote_builders,
        })
    }

    pub(crate) fn cancel_builders(&self) {
        let _ = self.cancellation.try_send(());
        for remote in &self.remote_builders {
            let _ = remote.cancellation.try_send(());
        }
    }

    pub(crate) fn get_builder(&self, job: &NowJob) -> color_eyre::Result<&dyn NowBuilder> {
        if job.build_system == self.system
            && job
                .required_system_features
                .iter()
                .all(|feature| self.system_features.contains(feature))
        {
            Ok(self)
        } else {
            for builder in self.remote_builders.iter() {
                if builder.systems.contains(&job.build_system)
                    && builder
                        .required_features
                        .iter()
                        .all(|feature| job.required_system_features.contains(feature))
                    && job
                        .required_system_features
                        .iter()
                        .all(|feature| builder.system_features.contains(feature))
                {
                    return Ok(builder);
                }
            }
            Err(color_eyre::eyre::eyre!(
                "No builders match for job '{}' (buildSystem = {}, requiredSystemFeatures = {:?})",
                job.name,
                job.build_system,
                job.required_system_features,
            ))
        }
    }
}

#[async_trait(?Send)]
impl NowBuilder for LocalBuilder {
    fn acquire(&self) -> Lock<'_, channel::Receiver<()>> {
        self.cancellation_rx.lock()
    }

    fn get_name(&self) -> String {
        self.hostname.clone()
    }

    fn get_style(&self) -> owo_colors::Style {
        Style::new().blue()
    }

    fn checkout(&self) -> color_eyre::Result<(Option<Box<dyn CheckoutTask>>, PathBuf)> {
        match self.strategy {
            CheckoutStrategy::Default => Ok((None, std::env::current_dir()?)),
            CheckoutStrategy::None => {
                let tmpdir = temp_dir().join(format!("now-{}", uuid::Uuid::new_v4()));

                let mut command = Command::new("mkdir");
                command
                    .arg("-p")
                    .arg(&tmpdir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                Ok((
                    Some(Box::new(CommandCheckoutTask {
                        builder: self.get_name(),
                        child: command.spawn()?,
                    })),
                    tmpdir,
                ))
            }
        }
    }

    async fn copy_derivations(
        &self,
        _job: &NowJob,
        _cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        Ok(())
    }

    async fn realize_derivation(
        &self,
        derivation: &Path,
        cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<PathBuf> {
        let mut command = Command::new("nix-store");
        command
            .arg("--realise")
            .arg(derivation)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let result = smol::future::race(
            async {
                if cancellation.recv().await.is_ok() {
                    return Err(color_eyre::eyre::eyre!("Runner aborted"));
                }
                smol::future::pending::<color_eyre::Result<PathBuf>>().await
            },
            async {
                if child.status().await?.success() {
                    let mut stdout = child.stdout.take().ok_or(color_eyre::eyre::eyre!(""))?;
                    let mut buf = Vec::new();
                    stdout.read_to_end(&mut buf).await?;
                    Ok(PathBuf::from(OsStr::from_bytes(buf.trim_ascii())))
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to realize derivation '{}' locally",
                        derivation.to_string_lossy(),
                    ))
                }
            },
        )
        .await;
        let _ = child.kill();
        result
    }

    async fn download(
        &self,
        _downloads: &[PathBuf],
        _cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        Ok(())
    }

    fn run_derivation(
        &self,
        cwdir: &Path,
        derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)> {
        let mut command = Command::new(derivation.join("bin/now-step"));
        let (reader, writer) = std::io::pipe()?;
        command
            .current_dir(cwdir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .env_clear()
            .envs(&self.env)
            .envs(envs);
        Ok((command.spawn()?, reader))
    }

    async fn fetch_derivation(
        &self,
        _derivation: &Path,
        _cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        Ok(())
    }

    async fn undo_checkout(&self, path: &Path) -> color_eyre::Result<()> {
        match self.strategy {
            CheckoutStrategy::Default => Ok(()),
            CheckoutStrategy::None => {
                let mut command = Command::new("rm");
                command.arg("-rf").arg(path);

                let mut child = command.spawn()?;
                if child.status().await?.success() {
                    Ok(())
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to remove locally checked out directory"
                    ))
                }
            }
        }
    }
}
