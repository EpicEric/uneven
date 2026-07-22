{
  jobs = {
    a = { ... }: {
      steps = [ { run = "echo a"; } ];
    };
    b = { ... }: {
      steps = [ { run = "echo b"; } ];
    };
    c = { ... }: {
      steps = [ { run = "echo c; exit 1"; } ];
    };

    x = { ... }: {
      needs = [ "a" ];
      steps = [ { run = "echo x"; } ];
    };
    y = { ... }: {
      needs = [
        "a"
        "x"
      ];
      steps = [ { run = "echo y; exit 1"; } ];
    };
  };
}
