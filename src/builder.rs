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
    ffi::{OsStr, OsString},
    io::{PipeReader, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use crate::{CheckoutStrategy, workflow::UnevenJob};

pub(crate) fn get_builder(job: &UnevenJob) -> color_eyre::Result<Box<dyn UnevenBuilder>> {
    if matches_local_builder(job) {
        Ok(Box::new(LocalBuilder))
    } else if let Some(builder) = RemoteBuilder::get_remote_builder(job)? {
        Ok(Box::new(builder))
    } else {
        Err(color_eyre::eyre::eyre!(
            "No builders match for job '{}'",
            job.name
        ))
    }
}

fn matches_local_builder(job: &UnevenJob) -> bool {
    todo!("matched local builder")
}

pub(crate) trait UnevenBuilder {
    fn name(&self) -> String;

    fn checkout(&self, strategy: CheckoutStrategy) -> color_eyre::Result<PathBuf>;

    fn copy_derivations(&self, job: &UnevenJob) -> color_eyre::Result<()>;

    fn realize_derivation(&self, derivation: &Path) -> color_eyre::Result<PathBuf>;

    fn download(&self, downloads: &[&Path]) -> color_eyre::Result<()>;

    fn run_derivation(
        &self,
        cwdir: &Path,
        derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)>;

    fn fetch_derivation(&self, derivation: &Path) -> color_eyre::Result<()>;
}

pub(crate) struct LocalBuilder;

impl UnevenBuilder for LocalBuilder {
    fn name(&self) -> String {
        "local".into()
    }

    fn checkout(&self, strategy: CheckoutStrategy) -> color_eyre::Result<PathBuf> {
        match strategy {
            CheckoutStrategy::Default => Ok(std::env::current_dir()?),
        }
    }

    fn copy_derivations(&self, _job: &UnevenJob) -> color_eyre::Result<()> {
        Ok(())
    }

    fn realize_derivation(&self, derivation: &Path) -> color_eyre::Result<PathBuf> {
        let mut command = Command::new("nix-store");
        command.arg("--realise");
        command.arg(derivation);

        let output = command.output()?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to realize derivation '{}'",
                derivation.to_string_lossy()
            ));
        }

        Ok(PathBuf::from(OsStr::from_bytes(
            output.stdout.as_slice().trim_ascii(),
        )))
    }

    fn download(&self, _downloads: &[&Path]) -> color_eyre::Result<()> {
        Ok(())
    }

    fn run_derivation(
        &self,
        cwdir: &Path,
        mut derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)> {
        derivation.push("bin");
        derivation.push("uneven-step");

        let mut command = Command::new(&derivation);
        let (reader, writer) = std::io::pipe()?;
        command
            .current_dir(cwdir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .envs(envs);
        Ok((command.spawn()?, reader))
    }

    fn fetch_derivation(&self, _derivation: &Path) -> color_eyre::Result<()> {
        Ok(())
    }
}

pub(crate) struct RemoteBuilder {
    pub(crate) ssh_user: String,
    pub(crate) ssh_host: String,
}

impl RemoteBuilder {
    fn get_remote_builder(job: &UnevenJob) -> color_eyre::Result<Option<Self>> {
        todo!("get remote builder")
    }

    fn ssh_remote(&self) -> String {
        format!("ssh://{}@{}", self.ssh_user, self.ssh_host)
    }
}

impl UnevenBuilder for RemoteBuilder {
    fn name(&self) -> String {
        self.ssh_remote()
    }

    fn checkout(&self, strategy: CheckoutStrategy) -> color_eyre::Result<PathBuf> {
        match strategy {
            CheckoutStrategy::Default => todo!("copy files over to remote"),
        }
    }

    fn copy_derivations(&self, job: &UnevenJob) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command.arg(self.ssh_remote());
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

        let output = command.output()?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy '{}' derivations to {}",
                job.name,
                self.ssh_remote()
            ));
        }

        Ok(())
    }

    fn realize_derivation(&self, derivation: &Path) -> color_eyre::Result<PathBuf> {
        let mut command = Command::new("ssh");
        command.args([&self.ssh_remote(), "nix-store", "--realise"]);
        command.arg(derivation);

        let output = command.output()?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to realize derivation '{}' in {}",
                derivation.to_string_lossy(),
                self.ssh_remote()
            ));
        }

        Ok(PathBuf::from(OsStr::from_bytes(
            output.stdout.as_slice().trim_ascii(),
        )))
    }

    fn download(&self, downloads: &[&Path]) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
        ]);
        command.arg(self.ssh_remote());
        command.args(downloads);

        let output = command.output()?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy uploads to {}",
                self.ssh_remote()
            ));
        }

        Ok(())
    }

    fn run_derivation(
        &self,
        cwdir: &Path,
        mut derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)> {
        derivation.push("bin");
        derivation.push("uneven-step");

        todo!("run derivation on remote")
    }

    fn fetch_derivation(&self, derivation: &Path) -> color_eyre::Result<()> {
        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--from",
        ]);
        command.arg(self.ssh_remote());
        command.arg(derivation);

        let output = command.output()?;
        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to copy '{}' derivation from {}",
                derivation.to_string_lossy(),
                self.ssh_remote()
            ));
        }

        Ok(())
    }
}
