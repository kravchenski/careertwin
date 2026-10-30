{
  description = "CareerTwin: the interview window (Rust) and a shell to work on the project";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        # the release build, made to run on NixOS: nothing is compiled (x86_64 only: that is what a release holds)
        prebuilt = pkgs.callPackage ./nix/prebuilt.nix { };
        # the window compiled from this checkout (several minutes: whisper.cpp and every Rust crate)
        from-source = pkgs.callPackage ./nix/package.nix { };
        careertwin-overlay = if pkgs.stdenv.hostPlatform.system == "x86_64-linux" then prebuilt else from-source;
        default = careertwin-overlay;
      });

      apps = forAll (pkgs: rec {
        careertwin-overlay = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.careertwin-overlay}/bin/careertwin-overlay";
          meta.description = "The CareerTwin interview window";
        };
        default = careertwin-overlay;
      });

      devShells = forAll (pkgs: {
        default = pkgs.callPackage ./nix/shell.nix { };
      });

      # for `nixpkgs.overlays = [ careertwin.overlays.default ];`
      overlays.default = final: _prev: {
        careertwin-overlay =
          if final.stdenv.hostPlatform.system == "x86_64-linux" then
            final.callPackage ./nix/prebuilt.nix { }
          else
            final.callPackage ./nix/package.nix { };
      };
    };
}
