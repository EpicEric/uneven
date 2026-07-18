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

use std::{ffi::OsStr, io::Read, os::unix::ffi::OsStrExt, path::PathBuf};

use crate::{
    CheckoutStrategy,
    builder::{LocalBuilder, UnevenBuilder},
    environment::UnevenEnvironment,
    workflow::UnevenJob,
};

impl UnevenEnvironment {
    fn run_job(
        &mut self,
        builder: impl UnevenBuilder,
        job: UnevenJob,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        eprintln!("Running job '{}'...", &job.name);

        let cwdir = builder.checkout(strategy)?;
        builder.copy_derivations(&job)?;
        let mut teardown_stack = Vec::new();
        for (_, upload) in self.uploads.iter() {
            builder.fetch_derivation(upload)?;
        }

        let mut result = Ok(());
        for step in job.steps.iter() {
            let step_call: Box<dyn FnOnce() -> color_eyre::Result<()>> = Box::new(|| {
                if let Some(teardown_drv) = step.teardown_drv.as_ref() {
                    let teardown = builder.realize_derivation(teardown_drv)?;
                    teardown_stack.push((&step.name, teardown, &step.env));
                }
                let run = builder.realize_derivation(&step.run_drv)?;
                let mut child = builder.run_derivation(&cwdir, run, self.env_vars(&step.env)?)?;
                if let Some(upload_key) = step.upload_key.as_ref() {
                    let mut stdout = child.stdout.take().expect("stdout is piped");
                    let mut buf = Vec::new();
                    stdout.read_to_end(&mut buf)?;
                    self.uploads.insert(
                        upload_key.clone(),
                        PathBuf::from(OsStr::from_bytes(buf.trim_ascii())),
                    );
                } else {
                    // TODO
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
            let mut child = builder.run_derivation(&cwdir, teardown, self.env_vars(step_env)?)?;
            // TODO
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
        self.run_job(LocalBuilder, job, strategy)
    }

    pub(crate) fn run_jobs_remote(
        &mut self,
        jobs: Vec<UnevenJob>,
        strategy: CheckoutStrategy,
    ) -> color_eyre::Result<()> {
        todo!()
    }
}
