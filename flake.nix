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
            rustToolchain
            pkg-config
            openssl
            cmake

            # Fonts for plotters
            fontconfig
            fontconfig.dev
            dejavu_fonts
            liberation_ttf
            noto-fonts

            # Build dependencies
            capnproto
            
            # CPU frequency control tools
            linuxPackages.cpupower
            dmidecode
            lm_sensors
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