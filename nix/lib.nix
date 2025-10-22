lib: let
  inherit
    (builtins)
    map
    toString
    substring
    stringLength
    ;
  inherit
    (lib.strings)
    concatMapStringsSep
    toLower
    toUpper
    replaceStrings
    optionalString
    ;

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

  parsedPlugins = plugins: anyrunPackage:
    if plugins == null
    then []
    else
      map (
        entry:
          if lib.types.package.check entry
          then "${entry}/lib/lib${replaceStrings ["-"] ["_"] entry.pname}.so"
          else let
            path = "${anyrunPackage}/lib/${entry}";
          in
            if builtins.pathExists path
            then path
            else let
              path = "${anyrunPackage}/lib/lib${replaceStrings ["-"] ["_"] entry}.so";
            in
              if builtins.pathExists path
              then path
              else if lib.strings.hasPrefix "/" entry
              then entry
              else throw "Anyrun: Plugin ${entry} does not exist"
      )
      plugins;

  keybinds = keybinds:
    if keybinds == null
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
        keybinds
      }],
    '';
  keyboardMode = mode:
    {
      "exclusive" = "Exclusive";
      "on-demand" = "OnDemand";
    }
        .${
      mode
    };
in {inherit assertNumeric stringifyNumeric capitalize parsedPlugins keybinds keyboardMode;}
