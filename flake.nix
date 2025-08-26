{
  description = "High-performance RPC library using QUIC and Cap'n Proto";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        # Rust toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain + editor support
            rustToolchain
            rust-analyzer

            # Build tooling and native deps
            pkg-config
            openssl
            cmake
            llvm
            clang
            zlib
            capnproto
            protobuf
            liburing

            # Plotting/visualization
            gnuplot
            fontconfig
            fontconfig.dev
            dejavu_fonts
            liberation_ttf
            noto-fonts
            python3
            python3Packages.pip
            python3Packages.matplotlib

            # Script utilities
            jq
            coreutils
            bash

            # CPU frequency control tools
            linuxPackages.cpupower
            dmidecode
            lm_sensors
          ];

          nativeBuildInputs = with pkgs; [
            cargo-nextest
            cargo-llvm-cov
          ];

          shellHook = ''
            # Initialize fontconfig cache inside devShell
            export FONTCONFIG_PATH="${pkgs.fontconfig.out}/etc/fonts"
            # Set XDG_DATA_DIRS to include fonts; do not expand in Nix
            if [ -z "$XDG_DATA_DIRS" ]; then
              export XDG_DATA_DIRS="${pkgs.dejavu_fonts}/share:${pkgs.noto-fonts}/share:${pkgs.liberation_ttf}/share"
            else
              export XDG_DATA_DIRS="${pkgs.dejavu_fonts}/share:${pkgs.noto-fonts}/share:${pkgs.liberation_ttf}/share:$XDG_DATA_DIRS"
            fi
            fc-cache -f >/dev/null 2>&1 || true

            export RUST_BACKTRACE=1
            unset RUSTFLAGS # avoid cargo injecting bogus literal from nix develop

            # Prefer pkg-config for OpenSSL discovery
            if [ -z "$PKG_CONFIG_PATH" ]; then
              export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
            else
              export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            fi
            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"

            # If no CPU frequency control access, warn but allow dev shell to continue
            if [ ! -w "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor" ]; then
              echo "Warning: No write access to CPU frequency controls."
              echo "Run benchmarks with sudo for full frequency scaling support."
            fi
          '';
        };
      }
    );
}