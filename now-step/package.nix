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
  callPackage,
  lib,
  zig_0_16,
  stdenv,

  optimizeLevel ? "Debug",
}:
stdenv.mkDerivation {
  name = "now-step";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./src
      ./build.zig
      ./build.zig.zon
    ];
  };

  strictDeps = true;
  __structuredAttrs = true;

  nativeBuildInputs = [ zig_0_16 ];

  zigBuildFlags = [
    "--system"
    "${callPackage ./deps.nix { }}"
    "-Doptimize=${optimizeLevel}"
  ];

  dontUseZigCheck = true;

  meta = {
    name = "now-step";
    description = "Step runner for now";
    homepage = "https://github.com/EpicEric/now";
    license = lib.licenses.agpl3Plus;
    mainProgram = "now-step";
    platforms = lib.platforms.all;
  };
}
