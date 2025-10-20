{
  lib,
  # Dependencies for Anyrun
  makeWrapper,
  glib,
  rustPlatform,
  gtk4,
  gtk4-layer-shell,
  pkg-config,
  librsvg,
  cargo,
  rustc,
  # Runtime deps
  anyrun-provider,
  # Additional configuration arguments for the
  # derivation. By default, we should not build
  # any of the plugins.
  dontBuildPlugins ? true,
  lockFile,
  ...
}:
let
  inherit (builtins) fromTOML readFile;

  cargoToml = fromTOML (readFile ../../anyrun/Cargo.toml);
  pname = cargoToml.package.name;
  version = cargoToml.package.version;

  fs = lib.fileset;
  s = ../..;
in
rustPlatform.buildRustPackage {
  inherit pname version;

  src = fs.toSource {
    root = s;
    fileset = fs.unions [
      (s + /anyrun)
      (s + /anyrun-macros)
      (s + /anyrun-plugin)
      (s + /plugins)
      (s + /Cargo.toml)
      (s + /Cargo.lock)
    ];
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
  ];

  buildInputs = [
    glib
    gtk4
    librsvg
    gtk4-layer-shell
  ];

  cargoBuildFlags = if dontBuildPlugins then [ "-p ${pname}" ] else [ ];

  doCheck = true;
  checkInputs = [
    cargo
    rustc
  ];

  copyLibs = true;

  buildAndTestSubdir = if dontBuildPlugins then pname else null;

  CARGO_BUILD_INCREMENTAL = "false";
  RUST_BACKTRACE = "full";

  postFixup = ''
    wrapProgram $out/bin/anyrun \
      --set GDK_PIXBUF_MODULE_FILE "$(echo ${librsvg.out}/lib/gdk-pixbuf-2.0/*/loaders.cache)" \
      --prefix PATH ":" ${lib.makeBinPath [ anyrun-provider ]} --prefix ANYRUN_PLUGINS ":" $out/lib
  '';

  passthru = {
    inherit anyrun-provider;
  };

  meta = {
    description = "Wayland native, highly customizable runner";
    homepage = "https://github.com/anyrun-org/anyrun";
    license = [ lib.licenses.gpl3 ];
    mainProgram = "anyrun";
    maintainers = with lib.maintainers; [
      NotAShelf
      n3oney
    ];
  };
}
