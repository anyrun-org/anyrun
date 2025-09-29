{
  description = "A wayland native, highly customizable runner.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
    anyrun-provider = {
      url = "github:anyrun-org/anyrun-provider";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      flake-parts,
      systems,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ flake-parts.flakeModules.easyOverlay ];
      systems = import systems;

      perSystem =
        {
          self',
          config,
          pkgs,
          ...
        }:
        let
          inherit (pkgs) callPackage;
        in
        {
          packages =
            let
              lockFile = ./Cargo.lock;

              # Since all plugin derivations are called with the exact same arguments
              # it is possible to streamline calling packages with a single function
              # that takes name as an argument, and handles default inherits.
              mkPlugin =
                name:
                callPackage ./nix/packages/plugin.nix {
                  inherit inputs lockFile;
                  inherit name;
                };
            in
            {
              default = self'.packages.anyrun;

              # By default the anyrun package is built without any plugins
              # as per the `dontBuildPlugins` arg.
              anyrun = callPackage ./nix/packages/anyrun.nix { inherit inputs lockFile; };
              anyrun-with-all-plugins = callPackage ./nix/packages/anyrun.nix {
                inherit inputs lockFile;
                dontBuildPlugins = false;
              };

              # Expose each plugin as a separate package. This uses the mkPlugin function
              # to call the same derivation with same default inherits and the name of the
              # plugin every time.
              applications = mkPlugin "applications";
              dictionary = mkPlugin "dictionary";
              kidex = mkPlugin "kidex";
              nix-run = mkPlugin "nix-run";
              randr = mkPlugin "randr";
              rink = mkPlugin "rink";
              shell = mkPlugin "shell";
              stdin = mkPlugin "stdin";
              symbols = mkPlugin "symbols";
              translate = mkPlugin "translate";
              websearch = mkPlugin "websearch";
              niri-focus = mkPlugin "niri-focus";

              anyrun-provider = inputs.anyrun-provider.packages.${pkgs.system}.default;
            };

          # Set up an overlay from packages exposed by this flake
          overlayAttrs = config.packages;

          devShells = {
            default = pkgs.mkShell {
              inputsFrom = builtins.attrValues self'.packages;
              packages = with pkgs; [
                rustc
                gcc
                cargo
                clippy
                rustfmt
              ];
            };

            nix = pkgs.mkShellNoCC {
              packages = with pkgs; [
                alejandra # formatter
                statix # linter
                deadnix # dead-code finder
              ];
            };
          };

          # Provides the default formatter for 'nix fmt', which will format the
          # entire tree with Alejandra. The wrapper script is necessary due to
          # changes to the behaviour of Nix, which now encourages wrappers for
          # tree-wide formatting.
          formatter = pkgs.writeShellApplication {
            name = "nix3-fmt-wrapper";

            runtimeInputs = [
              pkgs.nixfmt
              pkgs.fd
            ];

            text = ''
              # Find Nix files in the tree and format them with Alejandra
              fd "$@" -t f -e nix -x nixfmt -q '{}'
            '';
          };

          # Provides checks to be built an ran on 'nix flake check'. They can also
          # be built individually with 'nix build' as described below.
          checks = {
            # Check if codebase is properly formatted.
            # This can be initiated with `nix build .#checks.<system>.nix-fmt`
            # or with `nix flake check`
            nix-fmt = pkgs.runCommand "nix-fmt-check" { nativeBuildInputs = [ pkgs.alejandra ]; } ''
              nixfmt --check ${self} < /dev/null | tee $out
            '';
          };
        };

      flake = {
        homeManagerModules = {
          anyrun = import ./nix/modules/home-manager.nix self;
          default = self.homeManagerModules.anyrun;
        };
      };
    };
}
