{
  jobs = {
    format = { pkgs, ... }: {
      name = "Fix formatting";
      steps = [
        {
          run = ''
            cargo fmt --all
            treefmt
          '';
          path = [
            pkgs.cargo
            pkgs.rustfmt
            pkgs.nixfmt-tree
          ];
        }
      ];
    };
  };
}
