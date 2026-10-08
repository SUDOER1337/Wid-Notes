self: { config, lib, pkgs, ... }:
let
  cfg = config.programs.wid;
  inherit (lib) mkIf mkEnableOption mkOption types mkMerge;
in {
  options.programs.wid = {
    enable = mkEnableOption "Wid";
    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.wid;
      defaultText = "wid";
      description = "Wid package to install";
    };
    stylix = {
      enable = mkEnableOption "Stylix theme integration" // {
        default = true;
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    { home.packages = [ cfg.package ]; }

    (mkIf cfg.stylix.enable {
      xdg.configFile."wid/theme.toml" = let
        colors = config.lib.stylix.colors;
        c = colors.withHashtag;
      in {
        text = ''
          [meta]
          name = "Wid (${config.stylix.base16Scheme.slug})"
          author = "stylix"

          [editor]
          background = "${c.base00}"
          foreground = "${c.base05}"
          cursor = "${c.base0D}"
          cursor_insert = "${c.base0D}"
          cursor_visual = "${c.base09}"
          cursor_width = 2.0
          selection_bg = "${c.base02}"
          line_numbers = "${c.base04}"
          line_numbers_active = "${c.base05}"
          gutter_bg = "${c.base00}"
          whitespace = "${c.base03}"

          [sidebar]
          background = "${c.base01}"
          foreground = "${c.base05}"
          active_bg = "${c.base0D}"
          active_fg = "${c.base00}"
          hover_bg = "${c.base02}"
          border = "${c.base03}"
          separator = "${c.base03}"

          [palette]
          background = "${c.base01}"
          foreground = "${c.base05}"
          selected_bg = "${c.base0D}"
          selected_fg = "${c.base00}"
          input_bg = "${c.base02}"
          border = "${c.base03}"

          [titlebar]
          background = "${c.base00}"
          foreground = "${c.base05}"
          button_hover = "${c.base02}"
          button_close_hover = "${c.base08}"

          [status]
          background = "${c.base0D}"
          foreground = "${c.base00}"

          [syntax]
          keyword = "${c.base0E}"
          string = "${c.base0B}"
          comment = "${c.base03}"
          function = "${c.base0A}"
          number = "${c.base09}"
          type = "${c.base0C}"
          reserved = "${c.base0D}"
          operator = "${c.base05}"
        '';
      };
    })
  ]);
}
