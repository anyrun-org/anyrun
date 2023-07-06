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
  rustfmt,
  cargo,
  rustc,
  lockFile,
  dontBuildPlugins ? true,
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ../anyrun/Cargo.toml);
in
  rustPlatform.buildRustPackage rec {
    name = cargoToml.package.name;
    pname = "anyrun";
    #inherit version;

    src = ../.;

    buildInputs = [
      pkg-config
      glib
      atk
      gtk3
      librsvg
      gtk-layer-shell
    ];

    cargoLock = {
      lockFile = lockFile;
    };

    checkInputs = [cargo rustc];

    nativeBuildInputs = [
      pkg-config
      makeWrapper
      rustfmt
      rustc
      cargo
    ];

    cargoBuildFlags =
      if dontBuildPlugins
      then ["-p ${name}"]
      else [];

    doCheck = true;
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";
    copyLibs = true;
    buildAndTestSubdir = if dontBuildPlugins then name else null;

    postInstall = ''
      wrapProgram $out/bin/anyrun \
        --set GDK_PIXBUF_MODULE_FILE "$(echo ${librsvg.out}/lib/gdk-pixbuf-2.0/*/loaders.cache)" \
        --prefix ANYRUN_PLUGINS : $out/lib
    '';

    meta = with lib; {
      description = "A wayland native, highly customizable runner.";
      homepage = "https://github.com/Kirottu/anyrun";
      license = with licenses; [gpl3];
      maintainers = [
        {
          email = "neo@neoney.dev";
          github = "n3oney";
          githubId = 30625554;
          name = "Michał Minarowski";
        }
        {
          email = "raf@notashelf.dev";
          github = "NotAShelf";
        }
      ];
    };
  }
