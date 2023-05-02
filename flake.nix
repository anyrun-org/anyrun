{
  description = "A wayland native, highly customizable runner.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    cargoToml = builtins.fromTOML (builtins.readFile ./anyrun/Cargo.toml);
    supportedSystems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
  in {
    overlay = final: prev: {
      "${cargoToml.package.name}" = final.callPackage ./. {};
    };

    packages = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [self.overlay];
      };
    in {
      "${cargoToml.package.name}" = pkgs."${cargoToml.package.name}";
    });

    defaultPackage = forAllSystems (system:
      (import nixpkgs {
        inherit system;
        overlays = [self.overlay];
      })
      ."${cargoToml.package.name}");

    checks = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          self.overlay
        ];
      };
    in {
      format =
        pkgs.runCommand "check-format"
        {
          buildInputs = with pkgs; [rustfmt cargo];
        } ''
          ${pkgs.rustfmt}/bin/cargo-fmt fmt --manifest-path ${./anyrun}/Cargo.toml -- --check
          ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt --check ${./anyrun}
          touch $out # it worked!
        '';
      "${cargoToml.package.name}" = pkgs."${cargoToml.package.name}";
    });
    devShell = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [self.overlay];
      };
    in
      pkgs.mkShell {
        inputsFrom = [
          pkgs."${cargoToml.package.name}"
        ];
        buildInputs = with pkgs; [
          rustfmt
          nixpkgs-fmt
        ];
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      });
  };
}
