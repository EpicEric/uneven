{
  system ? builtins.currentSystem,
  inputs ? import ../.tack,
  pkgs ? import inputs.nixpkgs { inherit system; },
}:
let
  inherit (pkgs) lib;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../nix
      ../src
      ../Cargo.toml
      ../Cargo.lock
    ];
  };

  cix = pkgs.rustPlatform.buildRustPackage {
    name = "cix";

    inherit src;
    cargoLock.lockFile = ../Cargo.lock;

    strictDeps = true;
    __structuredAttrs = true;

    nativeBuildInputs = [
      pkgs.installShellFiles
      pkgs.makeWrapper
    ];

    doCheck = false;

    postInstall = ''
      wrapProgram $out/bin/cix \
        --suffix PATH : ${lib.makeBinPath [ pkgs.nix ]}
    ''
    + lib.optionalString (pkgs.stdenv.buildPlatform.canExecute pkgs.stdenv.hostPlatform) ''
      installShellCompletion --cmd cix \
        --bash <($out/bin/cix completions bash) \
        --fish <($out/bin/cix completions fish) \
        --zsh <($out/bin/cix completions zsh)
    '';

    meta = {
      name = "cix";
      description = "Nix-based CI helper";
      homepage = "https://github.com/EpicEric/cix";
      license = lib.licenses.mit;
      mainProgram = "cix";
      platforms = lib.platforms.linux ++ lib.platforms.darwin;
    };
  };
in
{
  inherit cix;

  shell = pkgs.mkShell {
    packages = [
      pkgs.cargo
      pkgs.pkg-config
      pkgs.rustc
    ];
  };
}
