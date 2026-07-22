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
  installShellFiles,
  lib,
  makeWrapper,
  nix,
  openssh,
  rsync,
  rustPlatform,
  src,
  stdenv,
}:
rustPlatform.buildRustPackage {
  pname = "uneven";
  version = (lib.importTOML ../Cargo.toml).package.version;

  inherit src;
  cargoLock.lockFile = ../Cargo.lock;

  strictDeps = true;
  __structuredAttrs = true;

  nativeBuildInputs = [
    installShellFiles
    makeWrapper
  ];

  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/uneven \
      --suffix PATH : ${
        lib.makeBinPath [
          nix
          openssh
          rsync
        ]
      }
  ''
  + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd uneven \
      --bash <($out/bin/uneven completions bash) \
      --fish <($out/bin/uneven completions fish) \
      --zsh <($out/bin/uneven completions zsh)
  '';

  meta = {
    name = "uneven";
    description = "Nix-based distributed command runner";
    homepage = "https://github.com/EpicEric/uneven";
    license = lib.licenses.agpl3Plus;
    mainProgram = "uneven";
    platforms = lib.platforms.all;
  };
}
