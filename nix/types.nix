{ lib }:
let
  inherit (lib) types;

  step = types.submodule {
    options = {
      name = lib.mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Name of the step.";
      };
      shell = lib.mkOption {
        type = types.nullOr types.package;
        default = null;
        description = "The shell to use for this step.";
      };
      shellArgs = lib.mkOption {
        type = types.nullOr (types.listOf types.str);
        default = null;
        description = "Args passed to the shell used in this step.";
      };
      run = lib.mkOption {
        type = types.str;
        default = "";
        description = "Shell script to run on this step.";
      };
      teardown = lib.mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Shell script to run when tearing down this step, after every step, in reverse order.";
      };
      path = lib.mkOption {
        type = types.listOf types.package;
        default = [ ];
        description = "Packages added to the PATH of the script.";
      };
      env = lib.mkOption {
        type = types.attrsOf (
          types.either types.str (
            types.submodule {
              options = {
                __cixSecret = lib.mkOption { type = types.str; };
              };
            }
          )
        );
        default = { };
        description = "Environment values to make available to the script.";
      };
    };
  };

  job = types.submodule {
    options = {
      name = lib.mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Name of the job.";
      };
      strategy = lib.mkOption {
        type = types.nullOr (
          types.submodule {
            options = {
              fail-fast = lib.mkOption {
                type = types.bool;
                default = true;
                description = "Whether a single failing run should cancel the remaining jobs in the matrix.";
              };
            };
          }
        );
        default = null;
      };
      needs = lib.mkOption {
        type = types.nullOr (types.listOf types.str);
        default = null;
        description = "Jobs that must be completed before running this one.";
      };
      steps = lib.mkOption {
        type = types.listOf (types.nullOr step);
        default = [ ];
        description = "Steps to run in this job.";
      };
    };
  };
in
{
  inherit step job;
}
