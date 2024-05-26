flake: final: prev: let
  anyrunPkgs = flake.packages.${final.system};
in {
  inherit (anyrunPkgs) anyrun anyrun-with-all-plugins;

  anyrunPlugins = {
    inherit
      (anyrunPkgs)
      applications
      dictionary
      kidex
      randr
      rink
      shell
      stdin
      symbols
      translate
      websearch
      ;
  };
}
