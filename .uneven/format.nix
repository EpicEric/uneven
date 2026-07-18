{ ... }:
{
  jobs = {
    format = { pkgs, ... }: {
      name = "Fix formatting";
      steps = [
        {
          name = "Run rustfmt";
          run = ''
            echo "Hello, world!"
            cargo fmt --all
            nixfmt .
          '';
          teardown = ''
            echo "Tearing down 'Fix formatting'..."
          '';
          path = [
            pkgs.cargo
            pkgs.rustfmt
            pkgs.nixfmt
          ];
        }
      ];
    };
  };
}
