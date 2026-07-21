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
    io::{PipeReader, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use owo_colors::Style;

use crate::{
    CheckoutStrategy,
    builder::{NixConfig, UnevenBuilder, remote::RemoteBuilder},
    environment::UnevenEnvironment,
    workflow::UnevenJob,
};

pub(crate) struct LocalBuilder {
    pub(crate) env: HashMap<OsString, OsString>,
    pub(crate) strategy: CheckoutStrategy,
    pub(crate) hostname: String,
    pub(crate) system: String,
    pub(crate) system_features: HashSet<String>,
    pub(crate) remote_builders: Vec<RemoteBuilder>,
}

impl LocalBuilder {
    pub(crate) fn new(
        environment: &UnevenEnvironment,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<Self> {
        let output = Command::new("nix")
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

        Ok(Self {
            env: environment.local_env.clone(),
            strategy,
            hostname: sys_info::hostname()?,
            system: config.system.value,
            system_features: config.system_features.value.into_iter().collect(),
            remote_builders,
        })
    }

    pub(crate) fn get_builder(&self, job: &UnevenJob) -> color_eyre::Result<&dyn UnevenBuilder> {
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

impl UnevenBuilder for LocalBuilder {
    fn get_name(&self) -> String {
        self.hostname.clone()
    }

    fn get_style(&self) -> owo_colors::Style {
        Style::new().blue()
    }

    fn checkout(&self) -> color_eyre::Result<PathBuf> {
        match self.strategy {
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
        derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)> {
        let mut command = Command::new(derivation.join("bin/uneven-step"));
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

    fn fetch_derivation(&self, _derivation: &Path) -> color_eyre::Result<()> {
        Ok(())
    }

    fn undo_checkout(&self, _path: &Path) -> color_eyre::Result<()> {
        match self.strategy {
            CheckoutStrategy::Default => Ok(()),
        }
    }
}
