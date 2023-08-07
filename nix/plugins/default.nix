{
  lib,
  glib,
  makeWrapper,
  rustPlatform,
  atk,
  gtk3,
  gtk-layer-shell,
  pkg-config,
  librsvg,
  inputs,
  name,
  lockFile,
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ../../plugins/${name}/Cargo.toml);
in
  rustPlatform.buildRustPackage {
    pname = cargoToml.package.name;
    version = cargoToml.package.version;

    src = "${inputs.self}";
    cargoLock = {
      inherit lockFile;
    };

    buildInputs = [
      glib
      atk
      gtk3
      librsvg
      gtk-layer-shell
    ];

    nativeBuildInputs = [
      pkg-config
      makeWrapper
    ];

    doCheck = true;
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";
    copyLibs = true;
    cargoBuildFlags = ["-p ${name}"];
    buildAndTestSubdir = "plugins/${name}";

    meta = with lib; {
      description = "The ${name} plugin for Anyrun";
      homepage = "https://github.com/Kirottu/anyrun";
      license = with licenses; [gpl3];
    };
  }
