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

{
  system ? builtins.currentSystem,
  pkgs ? import <nixpkgs> { inherit system; },
  mkUneven ? pkgs: (import ./. { inherit pkgs; }).uneven,
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
          system-features = e.system-features or [ ];
        }
      ) jobVal
    else
      fn {
        job = normalizeJob (jobVal {
          inherit pkgs;
          inherit (pkgs) lib;
        });
        pkgs' = pkgs;
        system-features = [ ];
      };

  unevenConfig =
    module:
    builtins.toJSON (
      module.config
      // {
        jobs = builtins.mapAttrs (
          _: job':
          mapMaybeList (
            {
              job,
              pkgs',
              system-features,
            }:
            job
            // {
              buildSystem = pkgs'.stdenv.buildPlatform.system;
              hostSystem = pkgs'.stdenv.hostPlatform.system;
              inherit system-features;
              steps = map (
                step:
                let
                  inherit (pkgs')
                    writeShellApplication
                    writeTextFile
                    ;
                  script =
                    text:
                    writeTextFile {
                      name = "uneven-step-script";
                      text = ''
                        #! ${lib.getExe (if step.shell == null then pkgs'.bash else step.shell)} ${
                          lib.optionalString (step.shellArgs != null) (lib.escapeShellArgs step.shellArgs)
                        }
                        ${text}
                      '';
                      executable = true;
                    };
                in
                {
                  run = writeShellApplication {
                    name = "uneven-step";
                    runtimeInputs = [ (mkUneven pkgs') ] ++ step.path;
                    text = ''
                      uneven step \
                        --script ${script step.run} \
                        --env ${lib.strings.escapeShellArg (builtins.toJSON step.env)} \
                        ${lib.optionalString (step.name != null) "--name ${lib.strings.escapeShellArg step.name}"}
                    '';
                  };
                  teardown =
                    if step.teardown == null then
                      null
                    else
                      writeShellApplication {
                        name = "uneven-step-teardown";
                        runtimeInputs = [ (mkUneven pkgs') ] ++ step.path;
                        text = ''
                          uneven step \
                            --teardown \
                            --script ${script step.teardown} \
                            --env ${lib.strings.escapeShellArg (builtins.toJSON step.env)} \
                            ${lib.optionalString (step.name != null) "--name ${lib.strings.escapeShellArg step.name}"}
                        '';
                      };
                  env = step.env;
                }
              ) job.steps;
            }
          ) job'
        ) module.config.jobs;
      }
    );
in
workflow: env:
unevenConfig (
  lib.evalModules {
    class = "uneven";
    modules = [
      ./module.nix
      workflow
    ];
    specialArgs = {
      ci = {
        secrets = lib.genAttrs env.secrets (name: {
          __unevenSecret = name;
        });

        inherit (env) vars;

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
            system-features = v.system-features or [ ];
          }) variants;

        steps = {
          build =
            name: deriv:
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to ci.steps.build must be a derivation";
            {
              name = "uneven: Build ${if name == "" then deriv else name}";
              run = ''
                uneven build --derivation ${deriv}
              '';
            };

          upload =
            name: deriv:
            assert lib.assertMsg (name != "") "name argument to ci.steps.upload must not be empty";
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to ci.steps.upload must be a derivation";
            {
              name = "uneven: Upload ${name}";
              run = ''
                uneven upload --name ${lib.escapeShellArg name} --derivation ${deriv}
              '';
            };
        };
      };
    };
  }
)
