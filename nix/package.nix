# cix: A Nix-based CI helper
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
  src,
  lib,
  rustPlatform,
  installShellFiles,
  makeWrapper,
  nix,
  stdenv,
}:
rustPlatform.buildRustPackage {
  pname = "cix";
  version = (fromTOML (builtins.readFile (src + "/Cargo.toml"))).package.version;

  inherit src;
  cargoLock.lockFile = src + "/Cargo.lock";

  strictDeps = true;
  __structuredAttrs = true;

  nativeBuildInputs = [
    installShellFiles
    makeWrapper
  ];

  doCheck = false;

  postInstall = ''
    wrapProgram $out/bin/cix \
      --suffix PATH : ${lib.makeBinPath [ nix ]}
  ''
  + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd cix \
      --bash <($out/bin/cix completions bash) \
      --fish <($out/bin/cix completions fish) \
      --zsh <($out/bin/cix completions zsh)
  '';

  meta = {
    name = "cix";
    description = "Nix-based CI helper";
    homepage = "https://github.com/EpicEric/cix";
    license = lib.licenses.agpl3Plus;
    mainProgram = "cix";
    platforms = lib.platforms.all;
  };
}
