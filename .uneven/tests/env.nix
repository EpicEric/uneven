{ runner, ... }:
{
  jobs = {
    env =
      { ... }:
      let
        another_name = runner;
        obtuse = {
          spam = runner;
        };
      in
      {
        steps = [
          {
            env = {
              TEST = another_name.vars.MY_VAR;
              inherit (obtuse.spam.secrets) MY_SECRET;
            };
            run = ''
              echo "TEST: $TEST"
              echo "MY_SECRET: $MY_SECRET"
            '';
          }
        ];
      };
  };
}
