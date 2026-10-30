{
  lib,
  rustPlatform,
  callPackage,
  cmake,
  pkg-config,
  makeWrapper,
  copyDesktopItems,
  makeDesktopItem,
}:
let
  runtime = callPackage ./runtime.nix { };
  manifest = builtins.fromTOML (builtins.readFile ../overlay/Cargo.toml);
  root = ../.;
in
rustPlatform.buildRustPackage {
  pname = "careertwin-overlay";
  version = manifest.package.version;

  # only what the window is built from: the crate, the font it embeds, the icon
  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      ../overlay
      ../prep/assets/fonts
      ../packaging/careertwin-interview.png
    ];
  };

  cargoRoot = "overlay";
  buildAndTestSubdir = "overlay";
  cargoLock.lockFile = ../overlay/Cargo.lock;

  # whisper.cpp is built by cmake from the crate's build script; bindgen needs libclang and the C headers
  nativeBuildInputs = [
    cmake
    pkg-config
    rustPlatform.bindgenHook
    makeWrapper
    copyDesktopItems
  ];
  dontUseCmakeConfigure = true;

  # the tests draw windows and reach for a display, a sound card and the network: they run in CI and in the checkout
  doCheck = false;

  desktopItems = [
    (makeDesktopItem {
      name = "careertwin-interview";
      desktopName = "CareerTwin Interview";
      genericName = "Interview assistant";
      comment = "Live answers for online interviews from your own CV and ABOUT";
      exec = "careertwin-overlay";
      icon = "careertwin-interview";
      categories = [ "Utility" ];
      startupWMClass = "careertwin-interview";
      keywords = [
        "interview"
        "cv"
        "resume"
        "career"
      ];
    })
  ];

  postInstall = ''
    install -Dm644 packaging/careertwin-interview.png \
      $out/share/icons/hicolor/256x256/apps/careertwin-interview.png

    # what a compositor key runs to take a screenshot from any program
    cat > $out/bin/careertwin-interview-ctl <<EOF
    #!/bin/sh
    exec $out/bin/careertwin-overlay ctl "\$@"
    EOF
    chmod +x $out/bin/careertwin-interview-ctl
  '';

  postFixup = ''
    wrapProgram $out/bin/careertwin-overlay \
      --prefix LD_LIBRARY_PATH : ${runtime.libraryPath} \
      --suffix PATH : ${runtime.binPath}
  '';

  meta = {
    description = "The CareerTwin interview window: hears the call, writes what to say, answers the screen";
    homepage = "https://github.com/kravchenski/careertwin";
    license = lib.licenses.asl20;
    mainProgram = "careertwin-overlay";
    platforms = lib.platforms.linux;
  };
}
