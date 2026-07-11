{
  system ? builtins.currentSystem,
  inputs ? import ../.tack,
  pkgs ? import inputs.nixpkgs { inherit system; },
}:
let
  mkCix = pkgs: (import ./. { inherit pkgs; }).cix;

  inherit (pkgs) lib;
  inherit (import ./types.nix { inherit lib; }) job;

  mapMaybeList =
    fn: jobVal:
    let
      normalize =
        j:
        (lib.evalModules {
          modules = [
            { options.__job = lib.mkOption { type = job; }; }
            { __job = j; }
          ];
        }).config.__job;
    in
    if builtins.isList jobVal then
      map (
        e:
        fn {
          job = normalize e.job;
          pkgs' = e.pkgs';
        }
      ) jobVal
    else
      fn {
        job = normalize (jobVal {
          inherit pkgs;
          inherit (pkgs) lib;
        });
        pkgs' = pkgs;
      };

  cixConfig =
    module:
    builtins.toJSON (
      module.config
      // {
        jobs = builtins.mapAttrs (
          _: job':
          mapMaybeList (
            { job, pkgs' }:
            job
            // {
              inherit (pkgs'.stdenv.hostPlatform) system;
              steps = map (
                step:
                let
                  inherit (pkgs')
                    writeShellApplication
                    writeTextFile
                    ;
                  script = writeTextFile {
                    name = "cix-step-script";
                    text = ''
                      #! ${lib.getExe (if step.shell == null then pkgs'.bash else step.shell)} ${
                        lib.optionalString (step.shellArgs != null) (lib.escapeShellArgs step.shellArgs)
                      }
                      ${step.run}
                    '';
                    executable = true;
                  };
                in
                writeShellApplication {
                  name = "cix-step";
                  runtimeInputs = [ (mkCix pkgs') ] ++ step.path;
                  text = ''
                    cix step --script ${script} ${
                      lib.optionalString (step.name != null) "--name ${lib.strings.escapeShellArg step.name}"
                    }
                  '';
                }
              ) job.steps;
            }
          ) job'
        ) module.config.jobs;
      }
    );
in
workflow: env:
cixConfig (
  lib.evalModules {
    class = "cix";
    modules = [
      ./module.nix
      workflow
    ];
    specialArgs = {
      ci = {
        secrets = lib.genAttrs env.secrets (name: {
          __cixSecret = name;
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
            pkgs' = pkgs;
          }) variants;

        steps = {
          build =
            name: deriv:
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to ci.steps.build must be a derivation";
            {
              name = "cix: Build ${if name == "" then deriv else name}";
              run = ''
                cix build --derivation ${deriv}
              '';
            };

          upload =
            name: deriv:
            assert lib.assertMsg (name != "") "name argument to ci.steps.upload must not be empty";
            assert lib.assertMsg (lib.isStorePath deriv)
              "derivation argument to ci.steps.upload must be a derivation";
            {
              name = "cix: Upload ${name}";
              run = ''
                cix upload --name ${lib.escapeShellArg name} --derivation ${deriv}
              '';
            };
        };
      };
    };
  }
)
