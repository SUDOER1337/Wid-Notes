self: { config, lib, pkgs, ... }:
let
  cfg = config.programs.wid;
  inherit (lib) mkIf mkEnableOption mkOption types;
in {
  options.programs.wid = {
    enable = mkEnableOption "Wid";
    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.wid;
      defaultText = "wid";
      description = "Wid package to install";
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
