{
  ci,
  lib,
  ...
}:
let
  inputs = import ../.tack;

  rust-overlay = import (
    fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"
  );

  cix = pkgs: import ../. { inherit pkgs; };
in
{
  jobs = {
    build =
      ci.matrix
        [
          {
            name = "Linux AMD64";
            pkgs = import inputs.nixpkgs { system = "x86_64-linux"; };
          }
          {
            name = "Linux ARM64";
            pkgs = import inputs.nixpkgs { system = "aarch64-linux"; };
          }
          {
            name = "macOS";
            pkgs = import inputs.nixpkgs { system = "aarch64-darwin"; };
          }
        ]
        (
          { pkgs, name, ... }: {
            name = "Build on ${name}";
            steps = [ (ci.steps.build "cix" (cix pkgs)) ];
          }
        );

    rustfmt-msrv = { pkgs, ... }: {
      name = "Check rustfmt formatting on MSRV";
      steps = [
        {
          name = "Run rustfmt";
          run = ''
            cargo fmt --check --all
          '';
          path = [
            pkgs.cargo
          ];
        }
      ];
    };

    tests-nightly =
      ci.matrix
        [
          {
            name = "Linux ARM64";
            pkgs = import inputs.nixpkgs {
              system = "aarch64-linux";
              overlays = [ rust-overlay ];
            };
          }
          {
            name = "macOS";
            pkgs = import inputs.nixpkgs {
              system = "aarch64-darwin";
              overlays = [ rust-overlay ];
            };
          }
        ]
        (
          { pkgs, name, ... }:
          {
            name = "Run tests on nightly (${name})";
            strategy.fail-fast = false;
            steps = [
              {
                name = "Test";
                env = {
                  RUSTFLAGS = "-A dead_code -A unused_variables";
                };
                run = ''
                  cargo nextest run --no-fail-fast --verbose --locked
                '';
                path = [
                  (pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default))
                  pkgs.cargo-nextest
                ]
                ++ lib.optionals (pkgs.stdenv.hostPlatform.isDarwin) [ pkgs.lld ];
              }
            ];
          }
        );

    coverage-nightly =
      ci.matrix
        [
          {
            pkgs = import inputs.nixpkgs {
              system = "x86_64-linux";
              overlays = [ rust-overlay ];
            };
          }
        ]
        (
          { pkgs, ... }: {
            name = "Test coverage on nightly";
            steps = [
              {
                name = "Test with coverage";
                env = {
                  RUSTFLAGS = "-A dead_code -A unused_variables";
                };
                run = ''
                  cargo llvm-cov nextest --no-fail-fast --verbose --codecov --locked --output-path codecov.json
                '';
                path = [
                  (pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default))
                  pkgs.cargo-llvm-cov
                  pkgs.cargo-nextest
                ];
              }
              {
                name = "Upload coverage reports to Codecov";
                env = {
                  CODECOV_TOKEN = ci.secrets.CODECOV_TOKEN;
                };
                run = ''
                  codecovcli do-upload -f ./codecov.json --token "$CODECOV_TOKEN"
                '';
                path = [
                  pkgs.codecov-cli
                ];
              }
            ];
          }
        );

    build-docker =
      ci.matrix
        [
          {
            pkgs = import inputs.nixpkgs { system = "x86_64-linux"; };
          }
          {
            pkgs = import inputs.nixpkgs { system = "aarch64-linux"; };
          }
        ]
        (
          { pkgs, ... }: {
            name = "Build Docker (${pkgs.stdenv.hostPlatform.system})";
            needs = [
              "build"
              "rustfmt-msrv"
              "tests-nightly"
              "coverage-nightly"
            ];
            steps = [
              (ci.steps.upload "docker-${pkgs.stdenv.hostPlatform.system}" (
                pkgs.dockerTools.buildImage {
                  name = "cix";
                  tag = "latest";
                  config.Entrypoint = [ (lib.getExe (cix pkgs)) ];
                }
              ))
            ];
          }
        );

    push-docker = { pkgs, ... }: {
      name = "Build Docker";
      needs = [
        "build-docker"
      ];
      steps = [
        {
          name = "Login to DockerHub";
          env.DOCKERHUB_PUSH_TOKEN = ci.secrets.DOCKERHUB_PUSH_TOKEN;
          run = ''
            echo $DOCKERHUB_PUSH_TOKEN | docker login --password-stdin --username ${ci.vars.DOCKERHUB_USERNAME} docker.io
          '';
          teardown = ''
            docker logout docker.io
          '';
          path = [
            pkgs.docker
          ];
        }
        {
          name = "Login to GHCR";
          env = {
            GITHUB_TOKEN = ci.secrets.GITHUB_TOKEN;
          };
          run = ''
            echo $GITHUB_TOKEN | docker login --pasword-stdin --username ${ci.vars.GITHUB_USERNAME} ghcr.io
          '';
          teardown = ''
            docker logout ghcr.io
          '';
          path = [
            pkgs.docker
          ];
        }
        {
          name = "Push images";
          env = {
            TAGS = builtins.concatStringsSep " " (
              map ({ image, tag }: "${image}:${tag}") (
                lib.cartesianProduct {
                  image = [
                    "${ci.vars.DOCKERHUB_USERNAME}/cix"
                    "ghcr.io/${ci.vars.GITHUB_USERNAME}/cix"
                  ];
                  tag = [
                    "latest"
                    "main"
                  ];
                }
              )
            );
          };
          run = ''
            amd_image=$(cix download --name docker-x86_64-linux)
            arm_image=$(cix download --name docker-aarch64-linux)

            for TAG in $TAGS; do
              skopeo copy docker-archive:$amd_image "docker://$TAG-amd64"
              skopeo copy docker-archive:$arm_image "docker://$TAG-arm64"
              docker buildx imagetools create --tag "$TAG" "$TAG-amd64" "$TAG-arm64"
            done
          '';
          path = [
            pkgs.docker-buildx
            pkgs.skopeo
          ];
        }
      ];
    };
  };
}
