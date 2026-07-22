{ ... }:
let
  mkNow = pkgs: import ./. { inherit pkgs; };
in
{
  jobs = {
    test-env =
      { pkgs, ... }:
      {
        name = "Test environment";
        steps = [
          {
            env = {
              MY_VAR = "This is a variable";
              MY_SECRET = "This is a secret";
            };
            path = [
              (mkNow pkgs)
            ];
            run = ''
              now run .now/tests/env.nix
            '';
          }
        ];
      };

    test-error =
      { pkgs, ... }:
      {
        name = "Test error exit status";
        steps = [
          {
            path = [
              (mkNow pkgs)
            ];
            run = ''
              # Ensure the test evaluates just fine
              now run --eval .now/tests/error.nix

              now run .now/tests/error.nix || error_code=$?
              if [ "$error_code" -eq 0 ]; then
                echo "Test shouldn't have succeeded!"
                exit 1
              else
                echo ""
                echo "=== hint: this means the test works ==="
              fi
            '';
          }
        ];
      };

    test-upload =
      { pkgs, ... }:
      {
        name = "Test uploads";
        steps = [
          {
            path = [
              (mkNow pkgs)
            ];
            run = ''
              now run .now/tests/upload.nix
            '';
          }
        ];
      };

    test-vars =
      { pkgs, ... }:
      {
        name = "Test envvars";
        steps = [
          {
            env = {
              TEST_FIRST_VAR = "first var";
              TEST_FIRST_SECRET = "first secret";
              TEST_SECOND_VAR = "second var";
              TEST_SECOND_SECRET = "second secret";
            };
            path = [
              (mkNow pkgs)
            ];
            run = ''
              now run .now/tests/vars.nix
            '';
          }
        ];
      };
  };
}
