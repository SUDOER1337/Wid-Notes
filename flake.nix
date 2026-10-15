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

        # Like craneLib.cleanCargoSource, but keeps assets/icons/*.svg so the
        # build script can render the window icon inside the Nix sandbox.
        filteredSrc = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: type:
            craneLib.filterCargoSources path type
            || pkgs.lib.hasSuffix ".svg" (toString path);
        };

        commonArgs = {
          src = filteredSrc;
          strictDeps = true;
          nativeBuildInputs = with pkgs; [ pkg-config mold resvg ];
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

        # Libraries the final binary dlopen()s (wgpu → libvulkan, glow →
        # libEGL/libGL, winit → libwayland/libxkbcommon/libX11). Nix's
        # automatic RPATH fixup only tracks DT_NEEDED, so these would be
        # missing from the `nix build` output and only work inside the dev
        # shell (whose shellHook exports LD_LIBRARY_PATH).
        runtimeLibs = with pkgs; [
          libxcb
          libX11
          libXrandr
          libXcursor
          libXi
          libxkbcommon
          wayland
          libGL
          mesa
          vulkan-loader
        ];

        # Pin the loader/first-party drivers from this flake's nixpkgs. The
        # system drivers are built against a newer glibc than this pin, so
        # letting the loader discover them yields "GLIBC_2.44 not found" →
        # no adapter → eframe exits.
        vulkanIcds = pkgs.lib.concatStringsSep ":" [
          "${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json"
          "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json"
        ];
        eglVendorDir = "${pkgs.mesa}/share/glvnd/egl_vendor.d";

        widDesktop = pkgs.makeDesktopItem {
          name = "wid";
          exec = "wid";
          icon = "wid";
          comment = "Vim-like note editor";
          desktopName = "Wid";
          genericName = "Note editor";
          categories = [ "Utility" "TextEditor" ];
          startupNotify = false;
        };

        wid = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;

          nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.makeWrapper ];

          # Add the dlopen()'d libraries to the RPATH (instead of shipping an
          # LD_LIBRARY_PATH) and wrap the binary to point the loaders at the
          # pinned Mesa drivers, making the result self-contained.
          postFixup = ''
            patchelf --add-rpath "${pkgs.lib.makeLibraryPath runtimeLibs}" $out/bin/wid
            wrapProgram $out/bin/wid \
              --set VK_DRIVER_FILES "${vulkanIcds}" \
              --set __EGL_VENDOR_LIBRARY_DIRS "${eglVendorDir}"
          '';

          # Ship the launcher icon in every form the freedesktop spec expects:
          # a scalable SVG plus PNG fallbacks, and a .desktop entry pointing at
          # Icon=wid. Without this, Nix-installed Wid has no launcher at all.
          postInstall = ''
            install -Dm644 ${./assets/icons/icon.svg} \
              $out/share/icons/hicolor/scalable/apps/wid.svg
            for size in 16 32 48 64 128 256 512; do
              mkdir -p $out/share/icons/hicolor/"$size"x"$size"/apps
              resvg --width "$size" --height "$size" ${./assets/icons/icon.svg} \
                $out/share/icons/hicolor/"$size"x"$size"/apps/wid.png
            done
            install -Dm644 ${widDesktop}/share/applications/wid.desktop \
              $out/share/applications/wid.desktop
          '';
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
                    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (commonArgs.buildInputs ++ commonArgs.nativeBuildInputs ++ runtimeLibs)}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
                    export VK_DRIVER_FILES="${vulkanIcds}"
                    export __EGL_VENDOR_LIBRARY_DIRS="${eglVendorDir}"
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
