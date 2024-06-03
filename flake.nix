{
  description = "A wayland native, highly customizable runner.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [flake-parts.flakeModules.easyOverlay];
      systems = import inputs.systems;

      perSystem = {
        self',
        config,
        pkgs,
        ...
      }: let
        inherit (pkgs) callPackage;
      in {
        # provide the formatter for nix fmt
        formatter = pkgs.alejandra;

        devShells.default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self'.packages;

          packages = with pkgs; [
            alejandra # nix formatter
            rustfmt # rust formatter
            statix # lints and suggestions
            deadnix # clean up unused nix code
            rustc # rust compiler
            gcc
            cargo # rust package manager
            clippy # opinionated rust formatter
          ];
        };

        packages = let
          lockFile = ./Cargo.lock;

          # Since all plugin derivations are called with the exact same arguments
          # it is possible to streamline calling packages with a single function
          # that takes name as an argument, and handles default inherits.
          mkPlugin = name:
            callPackage ./nix/plugins/default.nix {
              inherit inputs lockFile;
              inherit name;
            };
        in {
          default = self'.packages.anyrun;
          anyrun = callPackage ./nix/default.nix {inherit inputs lockFile;};

          anyrun-with-all-plugins = pkgs.callPackage ./nix/default.nix {
            inherit inputs lockFile;
            dontBuildPlugins = false;
          };

          # Expose each plugin as a separate package. This uses the mkPlugin function
          # to call the same derivation with same default inherits and the name of the
          # plugin every time.
          applications = mkPlugin "applications";
          dictionary = mkPlugin "dictionary";
          kidex = mkPlugin "kidex";
          randr = mkPlugin "randr";
          rink = mkPlugin "rink";
          shell = mkPlugin "shell";
          stdin = mkPlugin "stdin";
          symbols = mkPlugin "symbols";
          translate = mkPlugin "translate";
          websearch = mkPlugin "websearch";
        };

        # Set up an overlay from packages exposed by this flake
        overlayAttrs = config.packages;
      };

      flake = {self, ...}: {
        homeManagerModules = {
          anyrun = import ./nix/hm-module.nix inputs.self;
          default = self.homeManagerModules.anyrun;
        };
      };
    };
}
