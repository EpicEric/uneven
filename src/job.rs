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
    ffi::OsStr,
    io::{BufRead, BufReader, Read},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use owo_colors::{OwoColorize, Style};

use crate::{
    CheckoutStrategy,
    builder::{LocalBuilder, RemoteBuilder, UnevenBuilder},
    environment::UnevenEnvironment,
    workflow::{UnevenJob, UnevenStepEnvVar},
};

impl UnevenEnvironment {
    fn run_job(
        &mut self,
        builder: impl UnevenBuilder,
        style: Style,
        job: UnevenJob,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        eprintln!("Running job '{}'...", &job.name);

        let cwdir = builder.checkout(strategy)?;
        builder.copy_derivations(&job)?;
        let mut teardown_stack = Vec::new();

        let mut result = Ok(());
        for step in job.steps.iter() {
            let step_call: Box<dyn FnOnce() -> color_eyre::Result<()>> = Box::new(|| {
                if let Some(teardown_drv) = step.teardown_drv.as_ref() {
                    let teardown = builder.realize_derivation(teardown_drv)?;
                    teardown_stack.push((&step.name, teardown, &step.env));
                }
                let run = builder.realize_derivation(&step.run_drv)?;
                let mut downloads: Vec<&Path> = Vec::new();
                for (_, env) in &step.env {
                    if let UnevenStepEnvVar::Download(download) = env {
                        if let Some(path) = self.uploads.get(&download.download_name) {
                            downloads.push(path);
                        } else {
                            return Err(color_eyre::eyre::eyre!(
                                "No upload named '{}'",
                                &download.download_name,
                            ));
                        }
                    }
                }
                builder.download(&downloads)?;
                let (mut child, mut reader) =
                    builder.run_derivation(&cwdir, run, self.env_vars_for_step(&step.env)?)?;
                if let Some(upload_key) = step.upload_key.as_ref() {
                    let mut buf = Vec::new();
                    reader.read_to_end(&mut buf)?;
                    let upload_path = PathBuf::from(OsStr::from_bytes(buf.trim_ascii()));
                    builder.fetch_derivation(&upload_path)?;
                    eprintln!(
                        "{} Uploaded {} ({})",
                        format!("| Run '{}' |", step.name).style(style),
                        upload_key,
                        upload_path.to_string_lossy(),
                    );
                    self.uploads.insert(upload_key.clone(), upload_path);
                } else {
                    for line in BufReader::new(reader).lines() {
                        if let Ok(line) = line {
                            eprintln!(
                                "{} {}",
                                format!("| Run '{}' |", step.name).style(style),
                                line,
                            );
                        }
                    }
                }
                let exit_status = child.wait()?;
                if exit_status.success() {
                    Ok(())
                } else {
                    Err(color_eyre::eyre::eyre!(
                        "Step '{}' failed with exit code {}",
                        &step.name,
                        exit_status
                    ))
                }
            });
            if let Err(error) = (step_call)() {
                result = Err(error);
                break;
            }
        }

        for (step_name, teardown, step_env) in teardown_stack.into_iter().rev() {
            let (mut child, reader) =
                builder.run_derivation(&cwdir, teardown, self.env_vars_for_step(step_env)?)?;
            for line in BufReader::new(reader).lines() {
                if let Ok(line) = line {
                    eprintln!(
                        "{} {}",
                        format!("| Teardown '{}' |", step_name).style(style),
                        line
                    );
                }
            }
            let exit_status = child.wait()?;
            if !exit_status.success() {
                return Err(color_eyre::eyre::eyre!(
                    "Teardown for step '{}' failed with exit code {}",
                    step_name,
                    exit_status
                ));
            }
        }

        result
    }

    pub(crate) fn run_job_local(
        &mut self,
        job: UnevenJob,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        self.run_job(LocalBuilder, Style::new().blue(), job, strategy)
    }

    pub(crate) fn run_jobs_remote(
        &mut self,
        jobs: Vec<UnevenJob>,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        let styles = [
            Style::new().yellow(),
            Style::new().magenta(),
            Style::new().green(),
            Style::new().cyan(),
            Style::new().purple(),
            Style::new().red(),
        ];
        let mut result = Ok(());
        for (job, style) in jobs.into_iter().zip(styles.iter().cycle()) {
            let fail_fast = job.strategy.is_none_or(|strategy| strategy.fail_fast);
            if let Err(error) = self.run_job(
                RemoteBuilder {
                    ssh_user: todo!(),
                    ssh_host: todo!(),
                },
                *style,
                job,
                strategy,
            ) {
                if fail_fast {
                    return Err(error);
                } else {
                    result = Err(error);
                }
            }
        }
        result
    }
}
