{
  runner,
  lib,
  ...
}:
let
  rust-overlay = import (
    fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"
  );

  mkUneven = pkgs: import ../. { inherit pkgs; };
in
{
  jobs = {
    build =
      runner.matrix
        [
          {
            name = "Linux AMD64";
            pkgs = import <nixpkgs> { system = "x86_64-linux"; };
          }
          {
            name = "Linux ARM64";
            pkgs = import <nixpkgs> { system = "aarch64-linux"; };
          }
          {
            name = "macOS";
            pkgs = import <nixpkgs> { system = "aarch64-darwin"; };
          }
        ]
        (
          { pkgs, name, ... }: {
            name = "Build on ${name}";
            steps = [ (runner.steps.build "uneven" (mkUneven pkgs)) ];
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
            pkgs.rustfmt
          ];
        }
      ];
    };

    tests-nightly =
      runner.matrix
        [
          {
            name = "Linux ARM64";
            pkgs = import <nixpkgs> {
              system = "aarch64-linux";
              overlays = [ rust-overlay ];
            };
          }
          {
            name = "macOS";
            pkgs = import <nixpkgs> {
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
      runner.matrix
        [
          {
            pkgs = import <nixpkgs> {
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
                  CODECOV_TOKEN = runner.secrets.CODECOV_TOKEN;
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
      runner.matrix
        [
          {
            pkgs = import <nixpkgs> { system = "x86_64-linux"; };
            system-features = [ "docker" ];
          }
          {
            pkgs = import <nixpkgs> { system = "aarch64-linux"; };
            system-features = [ "docker" ];
          }
          {
            pkgs = import <nixpkgs> { system = "aarch64-darwin"; };
            system-features = [ "docker" ];
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
              (lib.mkIf (pkgs.stdenv.hostPlatform.isLinux) (
                runner.steps.upload "docker-${pkgs.stdenv.hostPlatform.system}" (
                  pkgs.dockerTools.buildImage {
                    name = "uneven";
                    tag = "latest";
                    config.Entrypoint = [ (lib.getExe (mkUneven pkgs)) ];
                  }
                )
              ))
            ];
          }
        );

    push-docker =
      runner.matrix
        [
          {
            system-features = [ "docker" ];
          }
        ]
        (
          { pkgs, ... }: {
            name = "Push Docker";
            needs = [
              "build-docker"
            ];
            steps = [
              {
                name = "Login to DockerHub";
                env.DOCKERHUB_PUSH_TOKEN = runner.secrets.DOCKERHUB_PUSH_TOKEN;
                run = ''
                  echo $DOCKERHUB_PUSH_TOKEN | docker login --password-stdin --username ${runner.vars.DOCKERHUB_USERNAME} docker.io
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
                  GITHUB_TOKEN = runner.secrets.GITHUB_TOKEN;
                };
                run = ''
                  echo $GITHUB_TOKEN | docker login --pasword-stdin --username ${runner.vars.GITHUB_USERNAME} ghcr.io
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
                          "${runner.vars.DOCKERHUB_USERNAME}/uneven"
                          "ghcr.io/${runner.vars.GITHUB_USERNAME}/uneven"
                        ];
                        tag = [
                          "latest"
                          "main"
                        ];
                      }
                    )
                  );
                  AMD_IMAGE = runner.download "docker-x86_64-linux";
                  ARM_IMAGE = runner.download "docker-aarch64-linux";
                };
                run = ''
                  for TAG in $TAGS; do
                    skopeo copy docker-archive:$AMD_IMAGE "docker://$TAG-amd64"
                    skopeo copy docker-archive:$ARM_IMAGE "docker://$TAG-arm64"
                    docker buildx imagetools create --tag "$TAG" "$TAG-amd64" "$TAG-arm64"
                  done
                '';
                path = [
                  pkgs.docker-buildx
                  pkgs.skopeo
                ];
              }
            ];
          }
        );
  };
}
