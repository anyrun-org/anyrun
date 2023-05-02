# default.nix
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
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ./anyrun/Cargo.toml);
in
  rustPlatform.buildRustPackage {
    src = ./.;

    buildInputs = [
      pkg-config
      glib
      atk
      gtk3
      librsvg
      gtk-layer-shell
    ];

    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes = {
        "kidex-common-0.1.0" = "sha256-sPzCTK0gdIYkKWxrtoPJ/F2zrG2ZKHOSmANW2g00fSQ=";
      };
    };

    checkInputs = [cargo rustc];

    nativeBuildInputs = [
      pkg-config
      makeWrapper
      rustfmt
      rustc
      cargo
    ];

    doCheck = true;
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";
    copyLibs = true;

    name = cargoToml.package.name;
    version = cargoToml.package.version;

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
      ];
    };
  }
