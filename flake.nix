# This is an example flake.nix for a Switch project based on devkitA64.
# It will work on any devkitPro example with a Makefile out of the box.
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
      };
    in {
      devShells.default = pkgs.mkShell {
        name = "smov2-server-dev";
        buildInputs = with pkgs; [
          curlftpfs
          rustPlatform.bindgenHook
          rustc
          cargo
          rustfmt
          gcc
          rust-analyzer
          cmake
          clippy
        ];

        # Each package provides a shell hook that sets all necessary
        # environmental variables. This part is necessary, otherwise your build
        # system won't know where to find devkitPro. By setting these
        # variables we allow devkitPro's example Makefiles to work out of the box.
      };
    });
}
