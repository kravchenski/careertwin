# nix develop: everything to build and run the window from a checkout, and to work on the Python side.
{
  lib,
  mkShell,
  callPackage,
  cargo,
  rustc,
  clippy,
  rustfmt,
  rust-analyzer,
  cmake,
  pkg-config,
  rustPlatform,
  python3,
  uv,
  git,
}:
let
  runtime = callPackage ./runtime.nix { };
in
mkShell {
  packages = [
    cargo
    rustc
    clippy
    rustfmt
    rust-analyzer
    cmake
    pkg-config
    python3
    uv
    git
  ] ++ runtime.tools;

  # libclang and the C headers for the bindings of whisper.cpp
  nativeBuildInputs = [ rustPlatform.bindgenHook ];
  buildInputs = runtime.libraries;

  LD_LIBRARY_PATH = runtime.libraryPath;
  RUST_SRC_PATH = rustPlatform.rustLibSrc;

  shellHook = ''
    echo "CareerTwin: cd overlay && cargo run --release   (the window)"
    echo "            cd overlay && cargo test            (needs cmake: it is here)"
  '';
}
