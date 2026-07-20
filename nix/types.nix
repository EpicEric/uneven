# uneven: A Nix-based distributed command runner
# Copyright (C) 2026 Eric Rodrigues Pires
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU Affero General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for
# more details.
#
# You should have received a copy of the GNU Affero General Public License along
# with this program. If not, see <https://www.gnu.org/licenses/>.

{ lib }:
let
  inherit (lib) types;

  env = types.attrsOf (
    types.either types.str (
      types.attrTag {
        __unevenSecret = lib.mkOption { type = types.str; };
        __unevenDownload = lib.mkOption { type = types.str; };
      }
    )
  );

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
        type = env;
        default = { };
        description = "Environment values to make available to this step.";
      };
      __unevenUploadKey = lib.mkOption {
        type = types.nullOr types.str;
        default = null;
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
      env = lib.mkOption {
        type = env;
        default = { };
        description = "Environment values to make available to steps in this job.";
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
