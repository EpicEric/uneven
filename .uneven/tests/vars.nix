{ runner, ... }:
{
  jobs = {
    first = { pkgs, ... }: {
      steps = [
        {
          shell = pkgs.python3;
          env = {
            FIRST_VAR = runner.vars.TEST_FIRST_VAR;
            FIRST_SECRET = runner.secrets.TEST_FIRST_SECRET;
          };
          run = ''
            import os

            assert os.environ["FIRST_VAR"] == "first var"
            assert os.environ["FIRST_SECRET"] == "first secret"
            assert os.environ.get("SECOND_VAR") is None
            assert os.environ.get("SECOND_SECRET") is None

            print(f"FIRST_VAR = {os.environ["FIRST_VAR"]}")
            print(f"FIRST_SECRET = {os.environ["FIRST_SECRET"]}")
          '';
        }
      ];
    };

    second = { pkgs, ... }: {
      needs = [ "first" ];
      steps = [
        {
          shell = pkgs.python3;
          env = {
            SECOND_VAR = runner.vars.TEST_SECOND_VAR;
            SECOND_SECRET = runner.secrets.TEST_SECOND_SECRET;
          };
          run = ''
            import os

            assert os.environ["SECOND_VAR"] == "second var"
            assert os.environ["SECOND_SECRET"] == "second secret"
            assert os.environ.get("FIRST_VAR") is None
            assert os.environ.get("FIRST_SECRET") is None

            print(f"SECOND_VAR = {os.environ["SECOND_VAR"]}")
            print(f"SECOND_SECRET = {os.environ["SECOND_SECRET"]}")
          '';
        }
      ];
    };
  };
}
