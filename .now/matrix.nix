# NOTE: This expects you to have registered a remote runner with the "now" system feature
#
{ runner, ... }:
{
  jobs = {
    empty = runner.matrix [ ] (
      { ... }: {
        steps = [
          {
            run = ''
              echo "This shouldn't run!"
              exit 1
            '';
          }
        ];
      }
    );

    local = runner.matrix [ { } ] (
      { ... }: {
        steps = [
          {
            run = ''
              printf "Hello from localhost!\npwd: "
              pwd
            '';
          }
        ];
      }
    );

    local-2 = runner.matrix [ { } ] (
      { ... }: {
        needs = [ "local" ];
        steps = [
          {
            run = ''
              ls
            '';
          }
        ];
      }
    );

    remote = runner.matrix [ { requiredSystemFeatures = [ "now" ]; } ] (
      { ... }: {
        steps = [
          {
            run = ''
              printf "Hello from the remote!\npwd: "
              pwd
            '';
          }
        ];
      }
    );
  };
}
