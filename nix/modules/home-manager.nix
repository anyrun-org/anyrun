self: {
  config,
  lib,
  ...
}: let
  inherit
    (builtins)
    toJSON
    toString
    ;

  anyrunLib = import ../lib.nix lib;

  inherit (lib.modules) mkIf mkMerge;
  inherit (lib.lists) optional;
  inherit (lib.attrsets) mapAttrs' nameValuePair;
  inherit
    (lib.strings)
    optionalString
    ;
  inherit (lib.trivial) boolToString;

  inherit (anyrunLib) assertNumeric stringifyNumeric keyboardMode parsedPlugins capitalize keybinds;

  cfg = config.programs.anyrun;
in {
  imports = [(import ./options.nix self)];

  config = mkIf cfg.enable {
    assertions = [
      (assertNumeric cfg.config.width)
      (assertNumeric cfg.config.height)
      (assertNumeric cfg.config.x)
      (assertNumeric cfg.config.y)
    ];

    warnings =
      if cfg.config.plugins == null
      then [
        ''
          You haven't enabled any plugins. Anyrun will not show any results, unless you specify plugins with the --override-plugins flag.
          Add plugins to programs.anyrun.config.plugins, or set it to [] to silence the warning.
        ''
      ]
      else [];

    systemd.user.services.anyrun = mkIf cfg.daemon.enable {
      Unit = {
        Description = "Anyrun daemon";
        PartOf = "graphical-session.target";
        After = "graphical-session.target";
      };

      Service = {
        Type = "simple";
        ExecStart = "${lib.getExe cfg.package} daemon";
        Restart = "on-failure";
        KillMode = "process";
      };

      Install = {
        WantedBy = ["graphical-session.target"];
      };
    };

    home.packages = optional (cfg.package != null) cfg.package;

    xdg.configFile = mkMerge [
      (mapAttrs' (name: value: nameValuePair ("anyrun/" + name) value) cfg.extraConfigFiles)

      {
        "anyrun/config.ron".text = ''
          Config(
            x: ${stringifyNumeric cfg.config.x},
            y: ${stringifyNumeric cfg.config.y},
            width: ${stringifyNumeric cfg.config.width},
            height: ${stringifyNumeric cfg.config.height},
            hide_icons: ${boolToString cfg.config.hideIcons},
            ignore_exclusive_zones: ${boolToString cfg.config.ignoreExclusiveZones},
            layer: ${capitalize cfg.config.layer},
            keyboard_mode: ${keyboardMode cfg.config.keyboardMode},
            hide_plugin_info: ${boolToString cfg.config.hidePluginInfo},
            close_on_click: ${boolToString cfg.config.closeOnClick},
            show_results_immediately: ${boolToString cfg.config.showResultsImmediately},
            max_entries: ${
            if cfg.config.maxEntries == null
            then "None"
            else "Some(${toString cfg.config.maxEntries})"
          },
            plugins: ${toJSON (parsedPlugins cfg.config.plugins cfg.package)},
            provider: "${lib.getExe cfg.config.provider}",
            ${optionalString (cfg.config.extraLines != null) cfg.config.extraLines}
            ${keybinds cfg.config.keybinds}
          )
        '';
      }

      {
        "anyrun/style.css" = mkIf (cfg.extraCss != null) {
          text = cfg.extraCss;
        };
      }
    ];
  };
}
