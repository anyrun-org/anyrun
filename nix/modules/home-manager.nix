self:
{
  config,
  pkgs,
  lib,
  ...
}:
let
  inherit (builtins)
    map
    toJSON
    toString
    substring
    stringLength
    ;
  inherit (lib.modules) mkIf mkMerge;
  inherit (lib.options) mkOption mkEnableOption literalExpression;
  inherit (lib.lists) optional;
  inherit (lib.attrsets) mapAttrs' nameValuePair;
  inherit (lib.strings)
    concatMapStringsSep
    toLower
    toUpper
    replaceStrings
    optionalString
    ;
  inherit (lib.trivial) boolToString;
  inherit (lib.types)
    nullOr
    package
    submodule
    int
    float
    listOf
    either
    str
    enum
    lines
    bool
    attrs
    ;

  defaultPackage = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
  defaultProvider = self.packages.${pkgs.stdenv.hostPlatform.system}.anyrun-provider;
  cfg = config.programs.anyrun;
in
{
  meta.maintainers = with lib.maintainers; [
    n3oney
    NotAShelf
  ];

  options.programs.anyrun = {
    enable = mkEnableOption "anyrun";
    daemon.enable = mkOption {
      type = bool;
      default = true;
      description = ''
        Enable running Anyrun as a daemon, allowing for faster startup speed.

        NOTE: This is required for clipboard functionality.
      '';
    };

    package = mkOption {
      type = nullOr package;
      default = defaultPackage;
      defaultText = literalExpression ''
        anyrun.packages.''${pkgs.stdenv.hostPlatform.system}.default
      '';
      description = ''
        Anyrun package to use. Defaults to the one provided by the flake.
      '';
    };
    config =
      let
        mkNumericOption =
          {
            default,
            description,
            ...
          }:
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
      in
      {
        plugins = mkOption {
          type = nullOr (listOf (either package str));
          default = null;
          description = ''
            List of anyrun plugins to use. Can either be packages, absolute plugin paths, or strings.
          '';
        };

        provider = mkOption {
          type = package;
          default = defaultProvider;
          description = ''
            The program that is used for loading the plugins, and for the communcation with them.
          '';
        };

        x = mkNumericOption {
          default.fraction = 0.5;
          description = ''
            The horizontal position, adjusted so that { relative = 0.5; } always centers the runner.

            ${numericInfo}
          '';
        };

        y = mkNumericOption {
          default.fraction = 0.0;
          description = ''
            The vertical position, works the same as x.

            ${numericInfo}
          '';
        };

        width = mkNumericOption {
          default.absolute = 800;
          description = ''
            The width of the runner.

            ${numericInfo}
          '';
        };

        height = mkNumericOption {
          default.absolute = 0;
          description = ''
            The minimum height of the runner, the runner will expand to fit all the entries.

            ${numericInfo}
          '';
        };

        hideIcons = mkOption {
          type = bool;
          default = false;
          description = "Hide match and plugin info icons";
        };

        ignoreExclusiveZones = mkOption {
          type = bool;
          default = false;
          description = "ignore exclusive zones, eg. Waybar";
        };

        layer = mkOption {
          type = enum [
            "background"
            "bottom"
            "top"
            "overlay"
          ];
          default = "overlay";
          description = "Layer shell layer (background, bottom, top or overlay)";
        };

        hidePluginInfo = mkOption {
          type = bool;
          default = false;
          description = "Hide the plugin info panel";
        };

        closeOnClick = mkOption {
          type = bool;
          default = false;
          description = "Close window when a click outside the main box is received";
        };

        showResultsImmediately = mkOption {
          type = bool;
          default = false;
          description = "Show search results immediately when Anyrun starts";
        };

        maxEntries = mkOption {
          type = nullOr int;
          default = null;
          description = "Limit amount of entries shown in total";
        };

        keybinds = mkOption {
          type = nullOr (
            listOf (submodule {
              options = {
                ctrl = mkOption {
                  type = bool;
                  default = false;
                };
                alt = mkOption {
                  type = bool;
                  default = false;
                };
                key = mkOption {
                  type = str;
                  description = ''
                    Name of the GDK keysym.

                    A list of possible values can be found at [https://gitlab.gnome.org/GNOME/gtk/-/blob/main/gdk/gdkkeysyms.h]
                  '';
                };
                action = mkOption {
                  type = enum [
                    "close"
                    "select"
                    "up"
                    "down"
                  ];
                };
              };
            })
          );
          default = null;
        };

        extraLines = mkOption {
          type = nullOr lines;
          default = null;
          description = "Extra lines to add inside the `Config()` object";
        };
      };

    extraCss = mkOption {
      type = nullOr lines;
      default = null;
      description = ''
        Extra CSS lines to add to {file}`~/.config/anyrun/style.css`.
      '';
    };

    extraConfigFiles = mkOption {
      # unfortunately HM doesn't really export the type for files, but hopefully
      # hm will throw errors if the options are wrong here, so I'm being *very* loose
      type = attrs;
      default = { };
      description = ''
        Extra files to put in {file}`~/.config/anyrun`, a wrapper over {option}`xdg.configFile`.
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

  config = mkIf cfg.enable (
    let
      assertNumeric = numeric: {
        assertion =
          !(
            (numeric ? absolute && numeric.absolute != null) && (numeric ? fraction && numeric.fraction != null)
          );
        message = "Invalid numeric definition, you can only specify one of absolute or fraction.";
      };

      stringifyNumeric =
        numeric:
        if (numeric ? absolute && numeric.absolute != null) then
          "Absolute(${toString numeric.absolute})"
        else
          "Fraction(${toString numeric.fraction})";

      capitalize =
        string: toUpper (substring 0 1 string) + toLower (substring 1 ((stringLength string) - 1) string);

      parsedPlugins =
        if cfg.config.plugins == null then
          [ ]
        else
          map (
            entry:
            if lib.types.package.check entry then
              "${entry}/lib/lib${replaceStrings [ "-" ] [ "_" ] entry.pname}.so"
            else
              let
                path = "${cfg.package}/lib/${entry}";
              in
              if builtins.pathExists path then
                path
              else
                let
                  path = "${cfg.package}/lib/lib${replaceStrings [ "-" ] [ "_" ] entry}.so";
                in
                if builtins.pathExists path then
                  path
                else if lib.strings.hasPrefix "/" entry then
                  entry
                else
                  throw "Anyrun: Plugin ${entry} does not exist"
          ) cfg.config.plugins;

      keybinds =
        if cfg.config.keybinds == null then
          ""
        else
          ''
            keybinds: [
              ${
                concatMapStringsSep "\n" (x: ''
                  Keybind(
                    ${optionalString x.ctrl "ctrl: true,"}
                    ${optionalString x.alt "alt: true,"}
                    key: "${x.key}",
                    action: ${capitalize x.action},
                  ),
                '') cfg.config.keybinds
              }],
          '';
    in
    {
      assertions = [
        (assertNumeric cfg.config.width)
        (assertNumeric cfg.config.height)
        (assertNumeric cfg.config.x)
        (assertNumeric cfg.config.y)
      ];

      warnings =
        if cfg.config.plugins == null then
          [
            ''
              You haven't enabled any plugins. Anyrun will not show any results, unless you specify plugins with the --override-plugins flag.
              Add plugins to programs.anyrun.config.plugins, or set it to [] to silence the warning.
            ''
          ]
        else
          [ ];

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
          WantedBy = [ "graphical-session.target" ];
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
              hide_plugin_info: ${boolToString cfg.config.hidePluginInfo},
              close_on_click: ${boolToString cfg.config.closeOnClick},
              show_results_immediately: ${boolToString cfg.config.showResultsImmediately},
              max_entries: ${
                if cfg.config.maxEntries == null then "None" else "Some(${toString cfg.config.maxEntries})"
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
