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
    path::PathBuf,
    pin::Pin,
};

use futures::{TryStreamExt, stream::FuturesUnordered};
use owo_colors::OwoColorize;
use smol::stream::StreamExt;

use crate::{
    builder::{UnevenBuilder, local::LocalBuilder},
    environment::UnevenEnvironment,
    workflow::{UnevenJob, UnevenStepEnvVar},
};

impl UnevenEnvironment {
    async fn run_job(&self, builder: &dyn UnevenBuilder, job: UnevenJob) -> color_eyre::Result<()> {
        let _guard = builder.acquire().await;
        let style = builder.get_style();
        let runner = builder.get_name();
        eprintln!(
            "{} Running job '{}'...",
            format!("{}>", runner).style(style),
            &job.name
        );

        let cwdir = builder.checkout().await?;
        builder.copy_derivations(&job).await?;
        let mut teardown_stack = Vec::new();

        let mut result = Ok(());
        for step in job.steps.iter() {
            let step_call: Box<
                dyn FnOnce() -> Pin<Box<dyn Future<Output = color_eyre::Result<()>>>>,
            > = Box::new(|| {
                Box::pin(async {
                    if let Some(teardown_drv) = step.teardown_drv.as_ref() {
                        let teardown = builder.realize_derivation(teardown_drv).await?;
                        teardown_stack.push((&step.name, teardown, &step.env));
                    }
                    let run = builder.realize_derivation(&step.run_drv).await?;
                    let mut downloads: Vec<PathBuf> = Vec::new();
                    {
                        let uploads = self.uploads.lock().expect("not poisoned");
                        for env in step.env.values() {
                            if let UnevenStepEnvVar::Download(download) = env {
                                if let Some(path) = uploads.get(&download.download_name) {
                                    downloads.push(path.clone());
                                } else {
                                    return Err(color_eyre::eyre::eyre!(
                                        "No upload named '{}'",
                                        &download.download_name,
                                    ));
                                }
                            }
                        }
                    }
                    builder.download(&downloads).await?;
                    let (mut child, mut reader) = builder.run_derivation(
                        &cwdir,
                        run,
                        self.generate_env_vars_for_step(&step.env)?,
                    )?;
                    if let Some(upload_key) = step.upload_key.as_ref() {
                        let mut buf = Vec::new();
                        reader.read_to_end(&mut buf)?;
                        let upload_path = PathBuf::from(OsStr::from_bytes(buf.trim_ascii()));
                        builder.fetch_derivation(&upload_path).await?;
                        eprintln!(
                            "{} Uploaded {} ({})",
                            format!("{} step[{}]>", runner, step.name).style(style),
                            upload_key,
                            upload_path.to_string_lossy(),
                        );
                        self.uploads
                            .lock()
                            .expect("not poisoned")
                            .insert(upload_key.clone(), upload_path);
                    } else {
                        for line in BufReader::new(reader).lines() {
                            if let Ok(line) = line {
                                eprintln!(
                                    "{} {}",
                                    format!("{} step[{}]>", runner, step.name).style(style),
                                    line,
                                );
                            } else {
                                break;
                            }
                        }
                    }
                    let exit_status = child.status().await?;
                    if exit_status.success() {
                        Ok(())
                    } else {
                        Err(color_eyre::eyre::eyre!(
                            "Step '{}' failed ({})",
                            &step.name,
                            exit_status
                        ))
                    }
                })
            });
            if let Err(error) = (step_call)().await {
                result = Err(error);
                break;
            }
        }

        for (step_name, teardown, step_env) in teardown_stack.into_iter().rev() {
            let (mut child, reader) = builder.run_derivation(
                &cwdir,
                teardown,
                self.generate_env_vars_for_step(step_env)?,
            )?;
            for line in BufReader::new(reader).lines() {
                if let Ok(line) = line {
                    eprintln!(
                        "{} {}",
                        format!("{} step[{}]>", runner, step_name).style(style),
                        line
                    );
                } else {
                    break;
                }
            }
            let exit_status = child.status().await?;
            if !exit_status.success() {
                eprintln!(
                    "{} Teardown failed ({}); continuing",
                    format!("{} step[{}]>", runner, step_name).style(style),
                    exit_status
                );
                result = Err(color_eyre::eyre::eyre!(
                    "Teardown for step '{}' failed ({})",
                    step_name,
                    exit_status
                ));
            }
        }

        builder.undo_checkout(&cwdir).await?;

        result
    }

    pub(crate) fn run_job_single<'a>(
        &'a self,
        local_builder: &'a LocalBuilder,
        job: UnevenJob,
    ) -> Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>> {
        Box::pin(self.run_job(local_builder, job))
    }

    pub(crate) fn run_jobs_multiple<'a>(
        &'a self,
        local_builder: &'a LocalBuilder,
        jobs: Vec<UnevenJob>,
    ) -> color_eyre::Result<(
        Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>>,
        Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>>,
    )> {
        let fail_fast = FuturesUnordered::new();
        let no_fail_fast = FuturesUnordered::new();

        for job in jobs {
            let builder = local_builder.get_builder(&job)?;
            if job
                .strategy
                .as_ref()
                .is_none_or(|strategy| strategy.fail_fast)
            {
                fail_fast.push(self.run_job(builder, job));
            } else {
                no_fail_fast.push(self.run_job(builder, job));
            }
        }

        Ok((
            Box::pin(async {
                let mut stream = fail_fast.into_stream();
                while let Some(future) = stream.next().await {
                    future?;
                }
                Ok(())
            }),
            Box::pin(async {
                let mut result = Ok(());
                let mut stream = no_fail_fast.into_stream();
                while let Some(future) = stream.next().await {
                    result = result.and(future);
                }
                result
            }),
        ))
    }
}
