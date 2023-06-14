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
  inputs,
  name,
  lockFile,
  ...
}:
rustPlatform.buildRustPackage rec {
  inherit name;
  pname = name;

  src = "${inputs.self}/plugins/applications";
  cargoLock.lockFile = lockFile;

  buildInputs = [
    pkg-config
    glib
    atk
    gtk3
    librsvg
    gtk-layer-shell
  ];

  nativeBuildInputs = [
    pkg-config
    makeWrapper
    rustfmt
  ];

  postPatch = ''
    cp ${lockFile} Cargo.lock
  '';

  doCheck = true;
  CARGO_BUILD_INCREMENTAL = "false";
  RUST_BACKTRACE = "full";
  copyLibs = true;

  meta = with lib; {
    description = "The applications plugin for Anyrun";
    homepage = "https://github.com/Kirottu/anyrun";
    license = with licenses; [gpl3];
  };
}
