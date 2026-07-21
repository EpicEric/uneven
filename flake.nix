{
  description = "Nix-based distributed command runner";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      eachSystem =
        f:
        (builtins.foldl' (
          acc: system:
          let
            fSystem = f system;
          in
          builtins.foldl' (
            acc': attr:
            acc'
            // {
              ${attr} = (acc'.${attr} or { }) // fSystem.${attr};
            }
          ) acc (builtins.attrNames fSystem)
        ) { } systems);
    in
    eachSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        inherit (import ./nix { inherit system pkgs; }) uneven shell;
      in
      {
        packages.${system} = {
          default = self.packages.${system}.uneven;
          inherit uneven;
        };

        apps.${system}.default = {
          type = "app";
          program = pkgs.lib.getExe uneven;
          inherit (uneven) meta;
        };

        devShells.${system}.default = shell;
      }
    );
}
