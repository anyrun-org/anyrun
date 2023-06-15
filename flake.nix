{
  description = "A wayland native, highly customizable runner.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # project shells
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux"];

      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        ...
      }: let
        inherit (inputs.nixpkgs) lib;
        inherit (lib) getExe;
      in {
        # provide the formatter for nix fmt
        formatter = pkgs.alejandra;

        devShells.default = inputs'.devshell.legacyPackages.mkShell {
          name = "anyrun-shell";
          packages = with pkgs; [
            alejandra # nix formatter
            rustfmt # rust formatter
            statix # lints and suggestions
            deadnix # clean up unused nix code
          ];
        };

        packages = let
          lockFile = ./Cargo.lock;
        in rec {
          anyrun = pkgs.callPackage ./nix/default.nix {inherit inputs lockFile;};
          # alias nix build .# to anyrun
          default = anyrun;

          # expose each plugin as a package
          applications = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "applications";
          };

          dictionary = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "dictionary";
          };

          kidex = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "kidex";
          };

          randr = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "randr";
          };

          rink = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "rink";
          };

          shell = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "shell";
          };

          stdin = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "stdin";
          };

          symbols = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "symbols";
          };

          translate = pkgs.callPackage ./nix/plugins/default.nix {
            inherit inputs lockFile;
            name = "translate";
          };
        };

        checks = {
          format =
            pkgs.runCommand "check-format" {
              buildInputs = with pkgs; [
                rustfmt
                cargo
              ];
            } ''
              ${pkgs.rustfmt}/bin/cargo-fmt fmt --manifest-path ./anyrun/Cargo.toml -- --check
              ${getExe pkgs.alejandra} --check ./
              touch $out # it worked!
            '';
          "anyrun-format-check" = self'.packages.anyrun;
        };
      };

      flake = _: {
        # TODO: Make a NixOS module
        nixosModules.default = null;

        homeManagerModules.default = import ./nix/hm-module.nix inputs.self;
      };
    };
}
