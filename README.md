# uneven

A Nix-based distributed command runner.

## Creating workflows

```nix
{ runner, lib, ... }:
{
  name = "Optional name for the workflow";
  jobs = {
    # Jobs are a sequence of steps run on a single machine, in parallel with other jobs
    job-1 =
      { pkgs, ... }:
      {
        name = "Optional name for the job";
        steps = [
          {
            name = "Optional name for the step";
            # Script to run in this step
            run = ''
              python3 --version > file
            '';
            # Packages included in the PATH of the script
            path = [
              pkgs.python313
            ];
          }
          (lib.mkIf (false) {
            name = "Skipped job";
          })
          {
            # You can set environment variables for this step (or for the whole job)
            env = {
              FOO = "Hello!";
              # Runtime-specified variable (interpolation allowed)
              BAR = "${runner.vars.BAR} (copy)";
              # Runtime-specified secret (interpolation not allowed)
              BAZ = runner.secrets.BAZ;
            };
            # You can also specify which shell to use
            shell = pkgs.python313;
            run = ''
              import os
              print(os.environ["FOO"])
            '';
            # Teardown always gets run even if the next jobs fail
            teardown = ''
              # Any printed secrets get anonymized
              print(os.environ["BAZ"])
            '';
          }
          # Special step to build and upload the provided derivation to the Nix store of other runners
          (runner.upload "my-derivation" (
            pkgs.writeText "my-derivation.txt" pkgs.stdenv.hostPlatform.system
          ))
        ];
      };

    another-job =
      # Use runner.matrix to specify multiple remote jobs
      runner.matrix
      [
        {
          # Specify the target system(s) of the remote via pkgs
          pkgs = import <nixpkgs> { system = "aarch64-linux"; };
        }
        {
          # Specify the system features of the remote
          system-features = [ "kvm" ];
        }
        {
          # Specify other optional parameters
          spam = "with eggs";
        }
      ]
      (
        { pkgs, spam ? null, ... }:
        {
          name = "Matrix job (${if spam != null then spam else pkgs.stdenv.hostPlatform.system})";
          # Run all jobs in this matrix even if one fails
          strategy.fail-fast = false;
          # Establish that this job must run after another
          needs = [ "job-1" ];
          # Downloads the previous upload with the same name into the runner's Nix store
          env.DRV = runner.download "my-derivation";
          steps = [
            # Special step that simply builds the provided derivation
            (runner.build "some-name" pkgs.hello)
            {
              run = "echo $DRV";
            }
          ];
        }
      )
  };
}
```

## Running workflows

```bash
cix run .uneven/workflow.nix
# --- or ---
cix run --env-file .env .uneven/workflow.nix
```
