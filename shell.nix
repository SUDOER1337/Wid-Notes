{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  nativeBuildInputs = [ pkgs.pkg-config pkgs.glib ];
  buildInputs = [ pkgs.gtk3 ];
  shellHook = ''
    export PKG_CONFIG_PATH=${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" [ pkgs.gtk3 ]}''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}
  '';
}
