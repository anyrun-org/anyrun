self: {
  config,
  pkgs,
  lib,
  hm,
  ...
}: let
  cfg = config.programs.anyrun;

  defaultPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
in {
  meta.maintainers = with lib.maintainers; [n3oney NotAShelf];

  options.programs.anyrun = with lib; {
    enable = mkEnableOption "anyrun";

    package = mkOption {
      type = with types; nullOr package;
      default = defaultPackage;
      defaultText = lib.literalExpression ''
        anyrun.packages.''${pkgs.stdenv.hostPlatform.system}.default
      '';
      description = mdDoc ''
        Anyrun package to use. Defaults to the one provided by the flake.
      '';
    };
    config = let
      mkNumericOption = {
        default,
        description,
        ...
      }:
        with types;
          mkOption {
            inherit default description;
            example = ''
              { absolute = 200; };
              or
              { fraction = 0.4; };
            '';
            type = submodule {
              options = {
                absolute = mkOption {
                  type = nullOr int;
                  default = null;
                };
                fraction = mkOption {
                  type = nullOr float;
                  default = null;
                };
              };
            };
          };

      numericInfo = ''
        This is a numeric option - pass either `{ absolute = int; };` or `{ fraction = float; };`.
        when using `absolute` it sets the absolute value in pixels,
        when using `fraction`, it sets a fraction of the width or height of the full screen (depends on exclusive zones and the settings related to them) window
      '';
    in {
      plugins = mkOption {
        type = with types; nullOr (listOf (either package str));
        default = null;
        description = mdDoc ''
          List of anyrun plugins to use. Can either be packages, absolute plugin paths, or strings.
        '';
      };

      width = mkNumericOption {
        default.absolute = 800;
        description = mdDoc ''
          How wide the input box and results are.

          ${numericInfo}
        '';
      };

      position = mkOption {
        type = with types; enum ["top" "center"];
        default = "top";
        description = "Where Anyrun is located on the screen (top or center)";
      };

      verticalOffset = mkNumericOption {
        default.absolute = 0;
        description = ''
          How much the runner is shifted vertically.

          ${numericInfo}
        '';
      };

      hideIcons = mkOption {
        type = types.bool;
        default = false;
        description = "Hide match and plugin info icons";
      };

      ignoreExclusiveZones = mkOption {
        type = types.bool;
        default = false;
        description = "ignore exclusive zones, eg. Waybar";
      };

      layer = mkOption {
        type = with types; enum ["background" "bottom" "top" "overlay"];
        default = "overlay";
        description = "Layer shell layer (background, bottom, top or overlay)";
      };

      hidePluginInfo = mkOption {
        type = types.bool;
        default = false;
        description = "Hide the plugin info panel";
      };

      closeOnClick = mkOption {
        type = types.bool;
        default = false;
        description = "Close window when a click outside the main box is received";
      };

      showResultsImmediately = mkOption {
        type = types.bool;
        default = false;
        description = "Show search results immediately when Anyrun starts";
      };

      maxEntries = mkOption {
        type = with types; nullOr int;
        default = null;
        description = "Limit amount of entries shown in total";
      };
    };

    extraCss = lib.mkOption {
      type = lib.types.nullOr lib.types.lines;
      default = "";
      description = mdDoc ''
        Extra CSS lines to add to {file}`~/.config/anyrun/style.css`.
      '';
    };

    extraConfigFiles = lib.mkOption {
      # unfortunately HM doesn't really export the type for files, but hopefully
      # hm will throw errors if the options are wrong here, so I'm being *very* loose
      type = lib.types.attrs;
      default = {};
      description = mdDoc ''
        Extra files to put in `~/.config/anyrun`, a wrapper over `xdg.configFile`.
      '';
      example = ''
        programs.anyrun.extraConfigFiles."plugin-name.ron".text = '''
          Config(
            some_option: true,
          )
        '''
      '';
    };
  };

  config = lib.mkIf cfg.enable (let
    assertNumeric = numeric: {
      assertion = !((numeric ? absolute && numeric.absolute != null) && (numeric ? fraction && numeric.fraction != null));
      message = "Invalid numeric definition, you can only specify one of absolute or fraction.";
    };

    stringifyNumeric = numeric:
      if (numeric ? absolute && numeric.absolute != null)
      then "Absolute(${builtins.toString numeric.absolute})"
      else "Fraction(${builtins.toString numeric.fraction})";

    capitalize = string:
      lib.toUpper (builtins.substring 0 1 string) + lib.toLower (builtins.substring 1 ((builtins.stringLength string) - 1) string);

    parsedPlugins = builtins.map (entry:
      if lib.types.package.check entry
      then "${entry}/lib/lib${lib.replaceStrings ["-"] ["_"] entry.pname}.so"
      else entry)
    cfg.config.plugins;
  in {
    assertions = [(assertNumeric cfg.config.width) (assertNumeric cfg.config.verticalOffset)];

    warnings =
      if cfg.config.plugins == null
      then [
        ''
          You haven't enabled any plugins. Anyrun will not start, unless you specify plugins with the --override-plugins flag.
          Add plugins to programs.anyrun.config.plugins, or set it to [] to silence the warning.
        ''
      ]
      else [];

    home.packages = lib.optional (cfg.package != null) cfg.package;

    xdg.configFile = lib.mkMerge [
      (lib.mapAttrs'
        (name: value: lib.nameValuePair ("anyrun/" + name) value)
        cfg.extraConfigFiles)

      {
        "anyrun/config.ron".text = ''
          Config(
            width: ${stringifyNumeric cfg.config.width},
            position: ${capitalize cfg.config.position},
            vertical_offset: ${stringifyNumeric cfg.config.verticalOffset},
            hide_icons: ${lib.boolToString cfg.config.hideIcons},
            ignore_exclusive_zones: ${lib.boolToString cfg.config.ignoreExclusiveZones},
            layer: ${capitalize cfg.config.layer},
            hide_plugin_info: ${lib.boolToString cfg.config.hidePluginInfo},
            close_on_click: ${lib.boolToString cfg.config.closeOnClick},
            show_results_immediately: ${lib.boolToString cfg.config.showResultsImmediately},
            max_entries: ${
            if cfg.config.maxEntries == null
            then "None"
            else builtins.toString cfg.config.maxEntries
          },
            plugins: ${builtins.toJSON parsedPlugins},
          )
        '';
      }

      {
        "anyrun/style.css" = lib.mkIf (cfg.extraCss != null) {
          text = cfg.extraCss;
        };
      }
    ];
  });
}
