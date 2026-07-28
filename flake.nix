# Dev shell and package for the music backend (Rust) + Angular frontend.
# Enter the shell with: nix develop
# Build and run the server with: nix run
#
# Pure-Rust deps throughout — the DSP core is hand-written and the WAV codec is
# `hound`, so there is no native audio library to link against.
{
  description = "music — derive music from the structure of a voice";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
      lib = nixpkgs.lib;

      # What the build is allowed to see.
      #
      # Listed rather than excluded, so a new directory has to be added on
      # purpose before it can reach the store. `data/` is the reason: it holds
      # real recordings of a real voice, and the nix store is world-readable and
      # effectively permanent. A gitignore-style filter that admitted everything
      # by default would put them there quietly, and there is no taking that
      # back.
      src = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./src
          ./tests
          ./music-analysis
          ./music-mapping
          ./music-realisation
        ];
      };
    in {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "music";
          version = "0.1.0";
          inherit src;

          # Resolved from the committed lockfile, so a build gets the dependency
          # versions the dev shell gets rather than whatever is newest — the
          # same reason the lockfile is committed at all.
          cargoLock.lockFile = ./Cargo.lock;

          # **The build runs the tests**, which is most of why this exists. A
          # nix build compiles and runs entirely inside /nix/store on the
          # internal disk, so it needs nothing from `~/.cache/cargo` — the
          # shared target directory on the external volume, where executing a
          # freshly built binary hangs indefinitely in dyld. This is a way to
          # build, run *and* check the code without touching that volume.
          doCheck = true;

          meta = {
            description = "Derive music from the structure of a voice";
            mainProgram = "music";
          };
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rust-analyzer
            pkgs.rustfmt
            pkgs.clippy
            pkgs.nodejs_24 # Angular 22 frontend (frontend/)
          ];
        };
      });
    };
}
