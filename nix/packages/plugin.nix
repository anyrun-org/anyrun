{
  inputs,
  lib,
  # Common dependencies for the plugin
  glib,
  makeWrapper,
  rustPlatform,
  atk,
  gtk3,
  gtk-layer-shell,
  pkg-config,
  librsvg,
  cargo,
  rustc,
  # Generic args
  name,
  lockFile,
  extraInputs ? [], # allow appending buildInputs
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ../../plugins/${name}/Cargo.toml);
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
in
  rustPlatform.buildRustPackage {
    inherit pname version;

    src = builtins.path {
      path = lib.sources.cleanSource inputs.self;
      name = "${pname}-${version}";
    };
    cargoLock = {
      inherit lockFile;
    };

    strictDeps = true;

    nativeBuildInputs = [
      pkg-config
      makeWrapper
    ];

    buildInputs =
      [
        glib
        atk
        gtk3
        librsvg
        gtk-layer-shell
      ]
      ++ extraInputs;

    doCheck = true;
    checkInputs = [
      cargo
      rustc
    ];

    copyLibs = true;
    cargoBuildFlags = ["-p ${name}"];
    buildAndTestSubdir = "plugins/${name}";

    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";

    meta = {
      description = "The ${name} plugin for Anyrun";
      homepage = "https://github.com/anyrun-org/anyrun";
      license = [lib.licenses.gpl3];
      maintainers = with lib.maintainers; [NotAShelf n3oney];
    };
  }
