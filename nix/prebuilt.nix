# The window as the release publishes it, made to run on NixOS without compiling anything.
#
# The Linux tarball of a release (built and started by CI) holds one program that needs only libc, libm,
# libgcc_s and libstdc++ at load time; autoPatchelfHook points it at the ones in the Nix store. The graphics,
# keyboard and window-system libraries it opens at run time are put on LD_LIBRARY_PATH by the same wrapper as in
# the package built from source. `release.json` names the release and the checksum of its tarball: the release
# workflow rewrites it after each release, so this follows the latest one.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  copyDesktopItems,
  makeDesktopItem,
  callPackage,
}:
let
  runtime = callPackage ./runtime.nix { };
  release = builtins.fromJSON (builtins.readFile ./release.json);
in
stdenv.mkDerivation {
  pname = "careertwin-overlay";
  inherit (release) version;

  src = fetchurl {
    url = "https://github.com/kravchenski/careertwin/releases/download/v${release.version}/CareerTwin-Interview-v${release.version}-linux-x86_64.tar.gz";
    inherit (release) hash;
  };
  sourceRoot = "CareerTwin-Interview";

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
    copyDesktopItems
  ];
  # libstdc++ and libgcc_s (whisper.cpp is C++); the build fails if anything else is left unresolved
  buildInputs = [ stdenv.cc.cc.lib ];

  dontConfigure = true;
  dontBuild = true;

  desktopItems = [ (makeDesktopItem (import ./desktop-item.nix)) ];

  installPhase = ''
    runHook preInstall
    install -Dm755 careertwin-overlay $out/bin/careertwin-overlay
    install -Dm644 careertwin-interview.png \
      $out/share/icons/hicolor/256x256/apps/careertwin-interview.png

    # what a compositor key runs to take a screenshot from any program
    cat > $out/bin/careertwin-interview-ctl <<EOF
    #!/bin/sh
    exec $out/bin/careertwin-overlay ctl "\$@"
    EOF
    chmod +x $out/bin/careertwin-interview-ctl
    runHook postInstall
  '';

  postFixup = ''
    wrapProgram $out/bin/careertwin-overlay \
      --prefix LD_LIBRARY_PATH : ${runtime.libraryPath} \
      --suffix PATH : ${runtime.binPath}
  '';

  meta = {
    description = "The CareerTwin interview window, from the release build";
    homepage = "https://github.com/kravchenski/careertwin";
    license = lib.licenses.asl20;
    mainProgram = "careertwin-overlay";
    platforms = [ "x86_64-linux" ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
