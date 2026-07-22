{
  jobs = {
    error = { ... }: {
      steps = [
        {
          run = ''
            exit 1
          '';
          teardown = ''
            echo ""
            echo "=== note: teardown still runs on error ==="
          '';
        }
      ];
    };
  };
}
