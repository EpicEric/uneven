{
  system ? builtins.currentSystem,
  inputs ? import ./.tack,
  pkgs ? import inputs.nixpkgs { inherit system; },
}:
(import ./nix {
  inherit
    system
    inputs
    pkgs
    ;
}).cix
