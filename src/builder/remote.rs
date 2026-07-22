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
    ffi::{OsStr, OsString},
    hash::{DefaultHasher, Hash, Hasher},
    io::PipeReader,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    pin::Pin,
};

use async_trait::async_trait;
use futures::FutureExt;
use owo_colors::Style;
use rand::{SeedableRng, seq::IndexedRandom};
use smol::{
    channel,
    io::{AsyncReadExt, AsyncWriteExt},
    lock::{Mutex, futures::Lock},
    process::{Child, Command, Stdio},
};

use crate::{
    CheckoutStrategy,
    builder::{CheckoutTask, CommandCheckoutTask, NixConfig, NowBuilder},
    utils::{escape_os_string, pipe_outputs_to_stderr},
    workflow::NowJob,
};

struct RsyncCheckoutTask {
    builder: String,
    child: Child,
    stdin_future: Pin<Box<dyn Future<Output = color_eyre::Result<()>>>>,
}

impl Drop for RsyncCheckoutTask {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl CheckoutTask for RsyncCheckoutTask {
    fn run<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>> {
        Box::pin(
            smol::future::zip(
                async {
                    let status = self.child.status().await?;
                    if status.success() {
                        Ok(())
                    } else {
                        pipe_outputs_to_stderr(&mut self.child).await?;
                        Err(color_eyre::eyre::eyre!(
                            "Failed to checkout current directory to {}",
                            self.builder
                        ))
                    }
                },
                &mut self.stdin_future,
            )
            .map(|(first, second)| first.and(second)),
        )
    }
}

pub(crate) struct RemoteBuilder {
    pub(crate) cancellation: channel::Sender<()>,
    pub(crate) cancellation_rx: Mutex<channel::Receiver<()>>,
    pub(crate) strategy: CheckoutStrategy,
    pub(crate) ssh_uri: String,
    pub(crate) ssh_identity: Option<String>,
    pub(crate) systems: HashSet<String>,
    pub(crate) system_features: HashSet<String>,
    pub(crate) required_features: HashSet<String>,
}

impl RemoteBuilder {
    pub(crate) fn get_remote_builders(
        config: &NixConfig,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<Vec<Self>> {
        let builders = if let Some(file) = config.builders.value.strip_prefix('@') {
            if !std::fs::exists(file)? {
                return Ok(vec![]);
            }
            String::from_utf8(std::fs::read(file)?)?
        } else {
            config.builders.value.clone()
        };

        let mut vec = vec![];
        for builder in regex::Regex::new(r"[\n;]+")
            .expect("valid regex")
            .split(&builders)
        {
            let mut iter = builder.split(' ');

            let Some(ssh_uri) = iter.next() else {
                continue;
            };

            let systems = if let Some(systems) = iter.next()
                && systems != "-"
            {
                systems
                    .split(',')
                    .map(|system| system.to_string())
                    .collect()
            } else {
                [config.system.value.clone()].into_iter().collect()
            };

            let ssh_identity = iter.next().and_then(|identity| {
                if identity == "-" {
                    None
                } else {
                    Some(identity.to_string())
                }
            });

            let _maximum_builds = iter.next();

            let _speed_factor = iter.next();

            let system_features = if let Some(system_features) = iter.next()
                && system_features != "-"
            {
                system_features
                    .split(',')
                    .map(|feature| feature.to_string())
                    .collect()
            } else {
                HashSet::new()
            };

            let required_features = if let Some(required_features) = iter.next()
                && required_features != "-"
            {
                required_features
                    .split(',')
                    .map(|feature| feature.to_string())
                    .collect()
            } else {
                HashSet::new()
            };

            let _ssh_host_key = iter.next();

            let (cancellation, cancellation_rx) = channel::bounded(1);

            vec.push(RemoteBuilder {
                cancellation,
                cancellation_rx: Mutex::new(cancellation_rx),
                strategy,
                ssh_uri: ssh_uri.to_string(),
                ssh_identity,
                systems,
                system_features,
                required_features,
            })
        }

        Ok(vec)
    }
}

#[async_trait(?Send)]
impl NowBuilder for RemoteBuilder {
    fn acquire(&self) -> Lock<'_, channel::Receiver<()>> {
        self.cancellation_rx.lock()
    }

    fn get_name(&self) -> String {
        self.ssh_uri.clone()
    }

    fn get_style(&self) -> owo_colors::Style {
        let mut hasher = DefaultHasher::new();
        self.ssh_uri.hash(&mut hasher);
        *[
            Style::new().yellow(),
            Style::new().magenta(),
            Style::new().green(),
            Style::new().cyan(),
            Style::new().purple(),
            Style::new().red(),
        ]
        .choose(&mut rand::rngs::SmallRng::seed_from_u64(hasher.finish()))
        .expect("not empty")
    }

    fn checkout(&self) -> color_eyre::Result<(Option<Box<dyn CheckoutTask>>, PathBuf)> {
        match self.strategy {
            CheckoutStrategy::Default => {
                let tmpdir = format!("now-{}", uuid::Uuid::new_v4());

                let mut command = Command::new("rsync");
                command
                    .args(["-arz", "--files-from=-", "."])
                    .arg(format!(
                        "{}:{}",
                        self.ssh_uri.strip_prefix("ssh://").unwrap_or(&self.ssh_uri),
                        tmpdir
                    ))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let mut child = command.spawn()?;
                let mut stdin = child.stdin.take().expect("stdin is piped");
                let stdin_future = Box::pin(async move {
                    let cwd = std::env::current_dir()?;
                    for dir_entry in ignore::Walk::new(std::env::current_dir()?).flatten() {
                        if dir_entry.file_type().is_some_and(|typ| typ.is_file()) {
                            let path = dir_entry.path();
                            stdin
                                .write_all(
                                    path.strip_prefix(&cwd)
                                        .unwrap_or(path)
                                        .as_os_str()
                                        .as_bytes(),
                                )
                                .await?;
                            stdin.write_all(b"\n").await?;
                        }
                    }
                    Ok(stdin.flush().await?)
                });

                Ok((
                    Some(Box::new(RsyncCheckoutTask {
                        builder: self.get_name(),
                        child,
                        stdin_future,
                    })),
                    PathBuf::from(tmpdir),
                ))
            }
            CheckoutStrategy::None => {
                let tmpdir = format!("now-{}", uuid::Uuid::new_v4());

                let mut command = Command::new("ssh");
                if let Some(ssh_identity) = self.ssh_identity.as_ref() {
                    command.arg("-i").arg(ssh_identity);
                }
                command
                    .args([&self.ssh_uri, "mkdir", &tmpdir])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                Ok((
                    Some(Box::new(CommandCheckoutTask {
                        builder: self.get_name(),
                        child: command.spawn()?,
                    })),
                    PathBuf::from(tmpdir),
                ))
            }
        }
    }

    async fn copy_derivations(
        &self,
        job: &NowJob,
        cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command.arg(&self.ssh_uri);
        command
            .args(
                job.steps
                    .iter()
                    .flat_map(|step| {
                        if let Some(teardown_drv) = step.teardown_drv.as_ref() {
                            vec![
                                step.run_drv.clone().into_os_string(),
                                teardown_drv.clone().into_os_string(),
                            ]
                        } else {
                            vec![step.run_drv.clone().into_os_string()]
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let result = smol::future::race(
            async {
                if cancellation.recv().await.is_ok() {
                    return Err(color_eyre::eyre::eyre!("Runner aborted"));
                }
                smol::future::pending::<color_eyre::Result<()>>().await
            },
            async {
                if child.status().await?.success() {
                    Ok(())
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to copy '{}' derivations to {}",
                        job.name,
                        self.ssh_uri
                    ))
                }
            },
        )
        .await;
        let _ = child.kill();
        result
    }

    async fn realize_derivation(
        &self,
        derivation: &Path,
        cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<PathBuf> {
        let mut command = Command::new("ssh");
        if let Some(ssh_identity) = self.ssh_identity.as_ref() {
            command.arg("-i").arg(ssh_identity);
        }
        command
            .args([&self.ssh_uri, "nix-store", "--realise"])
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
                        "Failed to realize derivation '{}' in {}",
                        derivation.to_string_lossy(),
                        self.ssh_uri
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
        downloads: &[PathBuf],
        cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command
            .arg(&self.ssh_uri)
            .args(downloads)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let result = smol::future::race(
            async {
                if cancellation.recv().await.is_ok() {
                    return Err(color_eyre::eyre::eyre!("Runner aborted"));
                }
                smol::future::pending::<color_eyre::Result<()>>().await
            },
            async {
                if child.status().await?.success() {
                    Ok(())
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to copy uploads to {}",
                        self.ssh_uri
                    ))
                }
            },
        )
        .await;
        let _ = child.kill();
        result
    }

    fn run_derivation(
        &self,
        cwdir: &Path,
        derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)> {
        let mut full_command: OsString = "cd ".into();
        full_command.push(cwdir);
        full_command.push(" ; ");

        for (key, value) in envs {
            full_command.push(key);
            full_command.push("=");
            full_command.push(escape_os_string(value));
            full_command.push(" ");
        }

        full_command.push(derivation.join("bin/now-step"));

        let mut command = Command::new("ssh");
        if let Some(ssh_identity) = self.ssh_identity.as_ref() {
            command.arg("-i").arg(ssh_identity);
        }
        command.arg(&self.ssh_uri).arg(full_command);
        let (reader, writer) = std::io::pipe()?;
        command
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);
        Ok((command.spawn()?, reader))
    }

    async fn fetch_derivation(
        &self,
        derivation: &Path,
        cancellation: &channel::Receiver<()>,
    ) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--from",
        ]);
        command.arg(&self.ssh_uri).arg(derivation);

        let mut child = command.spawn()?;
        let result = smol::future::race(
            async {
                if cancellation.recv().await.is_ok() {
                    return Err(color_eyre::eyre::eyre!("Runner aborted"));
                }
                smol::future::pending::<color_eyre::Result<()>>().await
            },
            async {
                if child.status().await?.success() {
                    Ok(())
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to copy '{}' derivation from {}",
                        derivation.to_string_lossy(),
                        self.ssh_uri
                    ))
                }
            },
        )
        .await;
        let _ = child.kill();
        result
    }

    async fn undo_checkout(&self, path: &Path) -> color_eyre::Result<()> {
        match self.strategy {
            CheckoutStrategy::Default | CheckoutStrategy::None => {
                let mut rm_command: OsString = "rm -rf ".into();
                rm_command.push(path);

                let mut command = Command::new("ssh");
                if let Some(ssh_identity) = self.ssh_identity.as_ref() {
                    command.arg("-i").arg(ssh_identity);
                }
                command.arg(&self.ssh_uri).arg(rm_command);

                let mut child = command.spawn()?;
                if child.status().await?.success() {
                    Ok(())
                } else {
                    pipe_outputs_to_stderr(&mut child).await?;
                    Err(color_eyre::eyre::eyre!(
                        "Failed to remove checked out directory in {}",
                        self.ssh_uri
                    ))
                }
            }
        }
    }
}
