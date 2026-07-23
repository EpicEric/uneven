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
  installShellFiles,
  lib,
  makeWrapper,
  nix,
  openssh,
  rsync,
  rustPlatform,
  stdenv,
}:
rustPlatform.buildRustPackage {
  pname = "now";
  version = (lib.importTOML ./Cargo.toml).package.version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./nix
      ./now-step
      ./src
      ./Cargo.toml
      ./Cargo.lock
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  strictDeps = true;
  __structuredAttrs = true;

  nativeBuildInputs = [
    installShellFiles
    makeWrapper
  ];

  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/now \
      --suffix PATH : ${
        lib.makeBinPath [
          nix
          openssh
          rsync
        ]
      }
  ''
  + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd now \
      --bash <($out/bin/now completions bash) \
      --fish <($out/bin/now completions fish) \
      --zsh <($out/bin/now completions zsh)
  '';

  meta = {
    name = "now";
    description = "Nix-based distributed command runner";
    homepage = "https://github.com/EpicEric/now";
    license = lib.licenses.agpl3Plus;
    mainProgram = "now";
    platforms = lib.platforms.all;
  };
}
