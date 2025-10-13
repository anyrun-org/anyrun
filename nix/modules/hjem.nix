self: {
  config,
  lib,
  ...
}: let
  inherit
    (builtins)
    map
    toJSON
    toString
    substring
    stringLength
    ;
  inherit (lib.modules) mkIf mkMerge;
  inherit (lib.lists) optional;
  inherit (lib.attrsets) mapAttrs' nameValuePair;
  inherit
    (lib.strings)
    concatMapStringsSep
    toLower
    toUpper
    replaceStrings
    optionalString
    ;
  inherit (lib.trivial) boolToString;

  cfg = config.programs.anyrun;
in {
  imports = [./options.nix];

  config = mkIf cfg.enable (
    let
      assertNumeric = numeric: {
        assertion =
          !(
            (numeric ? absolute && numeric.absolute != null) && (numeric ? fraction && numeric.fraction != null)
          );
        message = "Invalid numeric definition, you can only specify one of absolute or fraction.";
      };

      stringifyNumeric = numeric:
        if (numeric ? absolute && numeric.absolute != null)
        then "Absolute(${toString numeric.absolute})"
        else "Fraction(${toString numeric.fraction})";

      capitalize = string: toUpper (substring 0 1 string) + toLower (substring 1 ((stringLength string) - 1) string);

      parsedPlugins =
        if cfg.config.plugins == null
        then []
        else
          map (
            entry:
              if lib.types.package.check entry
              then "${entry}/lib/lib${replaceStrings ["-"] ["_"] entry.pname}.so"
              else let
                path = "${cfg.package}/lib/${entry}";
              in
                if builtins.pathExists path
                then path
                else let
                  path = "${cfg.package}/lib/lib${replaceStrings ["-"] ["_"] entry}.so";
                in
                  if builtins.pathExists path
                  then path
                  else if lib.strings.hasPrefix "/" entry
                  then entry
                  else throw "Anyrun: Plugin ${entry} does not exist"
          )
          cfg.config.plugins;

      keybinds =
        if cfg.config.keybinds == null
        then ""
        else ''
          keybinds: [
            ${
            concatMapStringsSep "\n" (x: ''
              Keybind(
                ${optionalString x.ctrl "ctrl: true,"}
                ${optionalString x.alt "alt: true,"}
                key: "${x.key}",
                action: ${capitalize x.action},
              ),
            '')
            cfg.config.keybinds
          }],
        '';
      keyboardMode =
        {
          "exclusive" = "Exclusive";
          "on-demand" = "OnDemand";
        }
        .${
          cfg.config.keyboardMode
        };
    in {
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

      systemd.services.anyrun = mkIf cfg.daemon.enable {
        description = "Anyrun daemon";
        script = "${lib.getExe cfg.package} daemon";
        partOf = ["graphical-session.target"];
        after = ["graphical-session.target"];
        wantedBy = ["graphical-session.target"];

        serviceConfig = {
          Type = "simple";
          Restart = "on-failure";
          KillMode = "process";
        };
      };

      packages = optional (cfg.package != null) cfg.package;

      xdg.config.files = mkMerge [
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
              keyboard_mode: ${keyboardMode},
              hide_plugin_info: ${boolToString cfg.config.hidePluginInfo},
              close_on_click: ${boolToString cfg.config.closeOnClick},
              show_results_immediately: ${boolToString cfg.config.showResultsImmediately},
              max_entries: ${
              if cfg.config.maxEntries == null
              then "None"
              else "Some(${toString cfg.config.maxEntries})"
            },
              plugins: ${toJSON parsedPlugins},
              provider: "${lib.getExe cfg.config.provider}",
              ${optionalString (cfg.config.extraLines != null) cfg.config.extraLines}
              ${keybinds}
            )
          '';
        }

        {
          "anyrun/style.css" = mkIf (cfg.extraCss != null) {
            text = cfg.extraCss;
          };
        }
      ];
    }
  );
}
