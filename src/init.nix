{ runner, lib, ... }:
{
  # name = "My workflow";
  jobs = {
    default =
      { pkgs, ... }:
      {
        # name = "My job";
        steps = [
          {
            # name = "My step";
            run = ''
              python3 -c 'print("Hello from now!")'
            '';
            path = [
              pkgs.python313
            ];
          }
        ];
      };
  };
}
