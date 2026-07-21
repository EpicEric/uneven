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
    ffi::{OsStr, OsString},
    hash::{DefaultHasher, Hash, Hasher},
    io::{PipeReader, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use owo_colors::Style;
use rand::{SeedableRng, seq::IndexedRandom};
use smol::{
    lock::Semaphore,
    process::{Child, Command, Stdio},
};

use crate::{
    CheckoutStrategy,
    builder::{NixConfig, UnevenBuilder},
    utils::escape_os_string,
    workflow::UnevenJob,
};

pub(crate) struct RemoteBuilder {
    pub(crate) strategy: CheckoutStrategy,
    pub(crate) semaphore: Semaphore,
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

            vec.push(RemoteBuilder {
                strategy,
                semaphore: Semaphore::new(1),
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
impl UnevenBuilder for RemoteBuilder {
    fn acquire(&self) -> smol::lock::futures::Acquire<'_> {
        self.semaphore.acquire()
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

    async fn checkout(&self) -> color_eyre::Result<PathBuf> {
        match self.strategy {
            CheckoutStrategy::Default => {
                let tmpdir = format!("uneven-{}", uuid::Uuid::new_v4());

                let files_to_copy: Vec<PathBuf> = ignore::Walk::new(std::env::current_dir()?)
                    .filter_map(|dir_entry| {
                        dir_entry.ok().and_then(|dir_entry| {
                            let pathbuf = dir_entry.into_path();
                            if pathbuf.is_file() {
                                Some(pathbuf)
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                let mut command = Command::new("rsync");
                command.arg("-az");
                for file in files_to_copy {
                    command.arg("--include").arg(file);
                }
                command.args(["--exclude='*'", "."]).arg(format!(
                    "{}:{}",
                    self.ssh_uri.strip_prefix("ssh://").unwrap_or(&self.ssh_uri),
                    tmpdir
                ));

                let output = command.output().await?;
                if !output.status.success() {
                    let mut stderr = std::io::stderr();
                    stderr.write_all(&output.stderr)?;
                    stderr.flush()?;
                    return Err(color_eyre::eyre::eyre!(
                        "Failed to checkout current directory to {}",
                        self.ssh_uri
                    ));
                }

                Ok(PathBuf::from(tmpdir))
            }
        }
    }

    async fn copy_derivations(&self, job: &UnevenJob) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command.arg(&self.ssh_uri);
        command.args(
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
        );

        let output = command.output().await?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy '{}' derivations to {}",
                job.name,
                self.ssh_uri
            ));
        }

        Ok(())
    }

    async fn realize_derivation(&self, derivation: &Path) -> color_eyre::Result<PathBuf> {
        let mut command = Command::new("ssh");
        if let Some(ssh_identity) = self.ssh_identity.as_ref() {
            command.arg("-i").arg(ssh_identity);
        }
        command
            .args([&self.ssh_uri, "nix-store", "--realise"])
            .arg(derivation);

        let output = command.output().await?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to realize derivation '{}' in {}",
                derivation.to_string_lossy(),
                self.ssh_uri
            ));
        }

        Ok(PathBuf::from(OsStr::from_bytes(
            output.stdout.as_slice().trim_ascii(),
        )))
    }

    async fn download(&self, downloads: &[PathBuf]) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command.arg(&self.ssh_uri).args(downloads);

        let output = command.output().await?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy uploads to {}",
                self.ssh_uri
            ));
        }

        Ok(())
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

        full_command.push(derivation.join("bin/uneven-step"));

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

    async fn fetch_derivation(&self, derivation: &Path) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--from",
        ]);
        command.arg(&self.ssh_uri).arg(derivation);

        let output = command.output().await?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy '{}' derivation from {}",
                derivation.to_string_lossy(),
                self.ssh_uri
            ));
        }

        Ok(())
    }

    async fn undo_checkout(&self, path: &Path) -> color_eyre::Result<()> {
        match self.strategy {
            CheckoutStrategy::Default => {
                let mut rm_command: OsString = "rm -rf ".into();
                rm_command.push(path);

                let mut command = Command::new("ssh");
                if let Some(ssh_identity) = self.ssh_identity.as_ref() {
                    command.arg("-i").arg(ssh_identity);
                }
                command.arg(&self.ssh_uri).arg(rm_command);

                let output = command.output().await?;
                if !output.status.success() {
                    let mut stderr = std::io::stderr();
                    stderr.write_all(&output.stderr)?;
                    stderr.flush()?;
                    return Err(color_eyre::eyre::eyre!(
                        "Failed to remove checked out directory in {}",
                        self.ssh_uri
                    ));
                }

                Ok(())
            }
        }
    }
}
