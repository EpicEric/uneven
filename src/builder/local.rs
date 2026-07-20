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
    workflow::UnevenJob,
};

pub(crate) struct LocalBuilder {
    pub(crate) hostname: String,
    pub(crate) system: String,
    pub(crate) system_features: HashSet<String>,
    pub(crate) remote_builders: Vec<RemoteBuilder>,
}

impl LocalBuilder {
    pub(crate) fn new() -> color_eyre::Result<Self> {
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

        let remote_builders = RemoteBuilder::get_remote_builders(&config)?;

        Ok(Self {
            hostname: sys_info::hostname()?,
            system: config.system.value,
            system_features: config.system_features.value.into_iter().collect(),
            remote_builders,
        })
    }

    pub(crate) fn get_builder(&self, job: &UnevenJob) -> color_eyre::Result<&dyn UnevenBuilder> {
        if job.build_system == self.system
            && job
                .system_features
                .iter()
                .all(|feature| self.system_features.contains(feature))
        {
            Ok(self)
        } else {
            for builder in self.remote_builders.iter() {
                if builder.systems.contains(&job.build_system)
                    && job
                        .system_features
                        .iter()
                        .all(|feature| builder.system_features.contains(feature))
                {
                    return Ok(builder);
                }
            }
            Err(color_eyre::eyre::eyre!(
                "No builders match for job '{}'",
                job.name
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
            .envs(envs);
        Ok((command.spawn()?, reader))
    }

    fn fetch_derivation(&self, _derivation: &Path) -> color_eyre::Result<()> {
        Ok(())
    }

    fn uncheckout(&self, strategy: CheckoutStrategy, _path: &Path) -> color_eyre::Result<()> {
        match strategy {
            CheckoutStrategy::Default => Ok(()),
        }
    }
}
