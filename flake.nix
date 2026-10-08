{
  description = "Wid - A local-first knowledge workspace";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    (flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config mold ];
          buildInputs = with pkgs; [
            gtk3
            glib
            cairo
            pango
            harfbuzz
            zlib
            atk
            gdk-pixbuf
            libxcb
            libX11
            libXrandr
            libXcursor
            libXi
            wayland
            libGL
            mesa
            vulkan-loader
            openssl.dev
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        wid = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        packages = {
          inherit wid;
          default = wid;
        };

        devShells.default = craneLib.devShell {
                  packages = with pkgs; [ just cargo-watch rust-analyzer mold ];
                  inherit (commonArgs) buildInputs nativeBuildInputs;
                  shellHook = ''
                    export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" (commonArgs.buildInputs ++ commonArgs.nativeBuildInputs)}:${pkgs.lib.makeSearchPathOutput "dev" "share/pkgconfig" (commonArgs.buildInputs ++ commonArgs.nativeBuildInputs)}''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
                    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (commonArgs.buildInputs ++ commonArgs.nativeBuildInputs)}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                    # Vulkan: force the shell's own mesa ICDs. The loader would
                    # otherwise find the system ones via XDG_DATA_DIRS, and those
                    # are built against a newer glibc than this nixpkgs pin →
                    # "GLIBC_2.44 not found" → no adapter → eframe exits.
                    export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json:${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json"
                  '';
                };
                devShells.mypure =
                          let
                            myBuildInputs = [
                              pkgs.openssl
                              pkgs.libx11
                              pkgs.libxcursor
                              pkgs.libxrandr
                              pkgs.libxi
                              pkgs.libxcb
                              pkgs.libxkbcommon
                              pkgs.vulkan-loader
                              pkgs.wayland
                              pkgs.libGL
                            ];
                          in pkgs.mkShell {
                            name = "wid-dev-mk";
                            packages = [ pkgs.rust-bin.stable.latest.default ];
                            buildInputs = myBuildInputs;
                            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath myBuildInputs;
                          };

        checks = {
          inherit wid;
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "-- --deny warnings";
          });
          fmt = craneLib.cargoFmt {
            src = craneLib.cleanCargoSource ./.;
          };
        };
      })) // {
      nixosModules = {
        wid = import ./nixos-module.nix self;
        default = self.nixosModules.wid;
      };
      homeManagerModules = {
        wid = import ./home-manager-module.nix self;
        default = self.homeManagerModules.wid;
      };
    };
}
