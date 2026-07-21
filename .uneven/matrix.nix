# NOTE: This expects you to have registered a remote runner with the "uneven" system feature
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

    remote = runner.matrix [ { requiredSystemFeatures = [ "uneven" ]; } ] (
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
