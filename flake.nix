# Dev shell for the music backend (Rust) + Angular frontend.
# Enter with: nix develop
# Pure-Rust deps throughout — the DSP core is hand-written and the WAV codec is
# `hound`, so there is no native audio library to link against.
{
  description = "music — derive music from the structure of a voice";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in {
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
