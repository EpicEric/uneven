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
    ffi::OsStr,
    io::{BufRead, BufReader, Read},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    pin::Pin,
};

use futures::{TryStreamExt, stream::FuturesUnordered};
use owo_colors::OwoColorize;
use smol::{channel::TryRecvError, stream::StreamExt};

use crate::{
    builder::{NowBuilder, local::LocalBuilder},
    environment::NowEnvironment,
    workflow::{NowJob, NowStepEnvVar},
};

type JobResult<'a> = Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>>;

impl NowEnvironment {
    async fn run_job(&self, builder: &dyn NowBuilder, job: NowJob) -> color_eyre::Result<()> {
        let guard = builder.acquire().await;
        let style = builder.get_style();
        let runner = builder.get_name();
        if matches!(guard.try_recv(), Ok(()) | Err(TryRecvError::Closed)) {
            return Err(color_eyre::eyre::eyre!("Runner aborted"));
        }
        eprintln!(
            "{} Running job '{}'...",
            format!("{}>", runner).style(style),
            &job.name
        );

        let (mut checkout_child, cwdir) = builder.checkout()?;

        let mut teardown_stack = Vec::new();

        let mut result = async {
            if let Some(checkout_child) = checkout_child.as_mut() {
                smol::future::race(
                    async {
                        let _ = guard.recv().await;
                        Err(color_eyre::eyre::eyre!("Runner aborted"))
                    },
                    checkout_child.run(),
                )
                .await?;
            }

            builder.copy_derivations(&job, &guard).await?;

            for step in job.steps.iter() {
                if let Some(teardown_drv) = step.teardown_drv.as_ref() {
                    let teardown = builder.realize_derivation(teardown_drv, &guard).await?;
                    teardown_stack.push((&step.name, teardown, &step.env));
                }
                let run = builder.realize_derivation(&step.run_drv, &guard).await?;
                let mut downloads: Vec<PathBuf> = Vec::new();
                {
                    let uploads = self.uploads.lock().expect("not poisoned");
                    for env in step.env.values() {
                        if let NowStepEnvVar::Download(download) = env {
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
                builder.download(&downloads, &guard).await?;
                let (mut child, mut reader) = builder.run_derivation(
                    &cwdir,
                    run,
                    self.generate_env_vars_for_step(&step.env)?,
                )?;
                if let Some(upload_key) = step.upload_key.as_ref() {
                    let mut buf = Vec::new();
                    reader.read_to_end(&mut buf)?;
                    let upload_path = PathBuf::from(OsStr::from_bytes(buf.trim_ascii()));
                    builder.fetch_derivation(&upload_path, &guard).await?;
                    eprintln!(
                        "{} Uploaded '{}' ({})",
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
                if !exit_status.success() {
                    return Err(color_eyre::eyre::eyre!(
                        "Step '{}' failed ({})",
                        &step.name,
                        exit_status
                    ));
                }
            }
            Ok(())
        }
        .await;

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
                result = result.and(Err(color_eyre::eyre::eyre!(
                    "Teardown for step '{}' failed ({})",
                    step_name,
                    exit_status
                )));
            }
        }

        drop(checkout_child.take());
        result.and(builder.undo_checkout(&cwdir).await)
    }

    pub(crate) fn run_job_single<'a>(
        &'a self,
        local_builder: &'a LocalBuilder,
        job: NowJob,
    ) -> Pin<Box<dyn Future<Output = color_eyre::Result<()>> + 'a>> {
        Box::pin(self.run_job(local_builder, job))
    }

    pub(crate) fn run_jobs_multiple<'a>(
        &'a self,
        local_builder: &'a LocalBuilder,
        jobs: Vec<NowJob>,
    ) -> color_eyre::Result<(JobResult<'a>, JobResult<'a>)> {
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
                let mut result = Ok(());
                let mut stream = fail_fast.into_stream();
                while let Some(future) = stream.next().await {
                    if future.is_err() && result.is_ok() {
                        local_builder.cancel_builders();
                        result = future;
                    }
                }
                result
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
