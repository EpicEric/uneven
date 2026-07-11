{ lib, ... }:
let
  inherit (lib) types;
in
{
  options = {
    name = lib.mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Name of the workflow";
    };
    jobs = lib.mkOption {
      type = types.attrsOf (types.nullOr types.raw);
      description = "Jobs in the workflow.";
    };
  };
}
