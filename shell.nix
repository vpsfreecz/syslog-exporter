{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  name = "syslog-exporter";
  packages = with pkgs; [ cargo rustc rustfmt clippy git python3 ];
}
