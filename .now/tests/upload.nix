{ runner, ... }:
let
  upload_key = "upload-key";
in
{
  jobs = {
    write = { pkgs, ... }: {
      steps = [
        (runner.steps.upload upload_key (pkgs.writeText "example" "Hello, world!"))
      ];
    };

    read = { ... }: {
      needs = [ "write" ];
      steps = [
        {
          env.FILE = runner.download upload_key;
          run = ''
            printf "$FILE = "
            cat $FILE
          '';
        }
      ];
    };
  };
}
