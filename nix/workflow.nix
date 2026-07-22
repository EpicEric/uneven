# now: A Nix-based distributed command runner
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

{
  system ? builtins.currentSystem,
  pkgs ? import <nixpkgs> { inherit system; },
  mkNow ? pkgs': (import ./. { pkgs = pkgs'; }).now,
}:

let
  inherit (pkgs) lib;
  inherit (import ./types.nix { inherit lib; }) job;

  normalizeJob =
    j:
    (lib.evalModules {
      modules = [
        { options.__job = lib.mkOption { type = job; }; }
        { __job = j; }
      ];
    }).config.__job;

  mapMaybeList =
    fn: jobVal:
    if builtins.isList jobVal then
      map (
        e:
        fn {
          job = normalizeJob e.job;
          pkgs' = e.pkgs' or pkgs;
          requiredSystemFeatures = e.requiredSystemFeatures or [ ];
        }
      ) jobVal
    else
      fn {
        job = normalizeJob (jobVal {
          inherit pkgs;
          inherit (pkgs) lib;
        });
        pkgs' = pkgs;
        requiredSystemFeatures = [ ];
      };

  stepFn =
    placeholder_name: pkgs': env: step:
    let
      inherit (pkgs')
        writeShellApplication
        writeTextFile
        ;
      script =
        text:
        writeTextFile {
          name = "now-step-script";
          text = ''
            #! ${lib.getExe (if step.shell == null then pkgs'.bash else step.shell)} ${
              lib.optionalString (step.shellArgs != null) (lib.escapeShellArgs step.shellArgs)
            }
            ${text}
          '';
          executable = true;
        };

      env' = builtins.mapAttrs (
        name: value:
        assert lib.assertMsg (lib.isValidPosixName name)
          "environment variable '${name}' is not a valid POSIX variable name";
        value
      ) (env // step.env);
    in
    {
      name = if (step.name != null && step.name != "") then step.name else placeholder_name;

      runDrv =
        (writeShellApplication {
          name = "now-step";
          runtimeInputs = step.path ++ [ (mkNow pkgs') ];
          text = ''
            now step \
              --derivation ${script step.run} \
              --env ${lib.strings.escapeShellArg (builtins.toJSON env')}
          '';
        }).drvPath;

      teardownDrv =
        if step.teardown == null then
          null
        else
          (writeShellApplication {
            name = "now-step";
            runtimeInputs = step.path ++ [ (mkNow pkgs') ];
            text = ''
              now step \
                --derivation ${script step.teardown} \
                --env ${lib.strings.escapeShellArg (builtins.toJSON env')}
            '';
          }).drvPath;

      env = env';

      __nowUploadKey = step.__nowUploadKey or null;
    };

  nowConfig =
    module:
    module.config
    // {
      jobs = builtins.mapAttrs (
        jobName: job':
        mapMaybeList (
          {
            job,
            pkgs',
            requiredSystemFeatures,
          }:
          assert lib.assertMsg (builtins.all (
            x: lib.isString x
          ) requiredSystemFeatures) "requiredSystemFeatures argument must be a list of strings";
          job
          // {
            name = if (job.name != null && job.name != "") then job.name else jobName;
            buildSystem = pkgs'.stdenv.buildPlatform.system;
            hostSystem = pkgs'.stdenv.hostPlatform.system;
            inherit requiredSystemFeatures;
            steps = lib.imap0 (i: stepFn "${jobName}-${toString i}" pkgs' job.env) job.steps;
          }
        ) job'
      ) module.config.jobs;
    };

  nowModule =
    { lib, ... }:
    let
      inherit (lib) types;
    in
    {
      options = {
        name = lib.mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Name of the workflow";
        };
        jobs = lib.mkOption {
          type = types.attrsOf (types.nullOr types.raw);
          description = "Jobs in the workflow.";
        };
      };
    };
in

workflow:
{ secrets, vars }:
nowConfig (
  lib.evalModules {
    class = "now";
    modules = [
      nowModule
      workflow
    ];
    specialArgs = {
      runner = {
        secrets = lib.genAttrs secrets (name: {
          __nowSecret = name;
        });

        inherit vars;

        matrix =
          variants: fn:
          map (v: {
            job = fn (
              {
                inherit pkgs;
                inherit (pkgs) lib;
              }
              // v
            );
            pkgs' = v.pkgs or pkgs;
            requiredSystemFeatures = v.requiredSystemFeatures or [ ];
          }) variants;

        steps = {
          build =
            name: deriv:
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to runner.steps.build must be a derivation";
            {
              name = "build ${if name == "" then deriv else name}";
              run = ''
                now build --derivation ${deriv}
              '';
            };

          upload =
            name: deriv:
            assert lib.assertMsg (name != "") "name argument to runner.steps.upload must not be empty";
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to runner.steps.upload must be a derivation";
            {
              name = "upload ${name}";
              run = ''
                now build --derivation ${deriv}
              '';
              __nowUploadKey = name;
            };
        };

        download = name: {
          __nowDownload = name;
        };
      };
    };
  }
)
