{
  inputs,
  lib,
  # Dependencies for Anyrun
  makeWrapper,
  glib,
  rustPlatform,
  gtk4,
  gtk4-layer-shell,
  pkg-config,
  librsvg,
  rustfmt,
  cargo,
  rustc,
  # Additional configuration arguments for the
  # derivation. By default, we should not build
  # any of the plugins.
  dontBuildPlugins ? true,
  lockFile,
  ...
}: let
  inherit (builtins) fromTOML readFile;

  cargoToml = fromTOML (readFile ../../anyrun/Cargo.toml);
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
in
  rustPlatform.buildRustPackage {
    inherit pname version;
    src = builtins.path {
      path = lib.sources.cleanSource inputs.self;
      name = "${pname}-${version}";
    };

    strictDeps = true;

    cargoLock = {
      inherit lockFile;
      # Temporary while packages aren't yet stabilized
      allowBuiltinFetchGit = true;
    };

    nativeBuildInputs = [
      pkg-config
      makeWrapper
      rustfmt
      rustc
      cargo
    ];

    buildInputs = [
      glib
      gtk4
      librsvg
      gtk4-layer-shell
    ];

    cargoBuildFlags =
      if dontBuildPlugins
      then ["-p ${pname}"]
      else [];

    doCheck = true;
    checkInputs = [
      cargo
      rustc
    ];

    copyLibs = true;

    buildAndTestSubdir =
      if dontBuildPlugins
      then pname
      else null;

    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";

    postInstall = ''
      wrapProgram $out/bin/anyrun \
        --set GDK_PIXBUF_MODULE_FILE "$(echo ${librsvg.out}/lib/gdk-pixbuf-2.0/*/loaders.cache)" \
        --prefix ANYRUN_PLUGINS : $out/lib
    '';

    meta = {
      description = "A wayland native, highly customizable runner.";
      homepage = "https://github.com/anyrun-org/anyrun";
      license = [lib.licenses.gpl3];
      mainProgram = "anyrun";
      maintainers = with lib.maintainers; [
        NotAShelf
        n3oney
      ];
    };
  }
