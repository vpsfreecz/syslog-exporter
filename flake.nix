{
  description = "Syslog Exporter development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/718f670e845328e9c2cecdd832835712b2cbd065";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
    in
    {
      packages = nixpkgs.lib.genAttrs systems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        { default = pkgs.callPackage ./package.nix { }; });

      devShells = nixpkgs.lib.genAttrs systems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            name = "syslog-exporter";
            packages = with pkgs; [ cargo rustc rustfmt clippy git python3 cargo-audit ];
          };
        });
    };
}
