{
  description = "Textamp: AI-modified terminal music player for Linux and macOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
      nativeInputs = pkgs: [ pkgs.pkg-config ];
      libraryInputs =
        pkgs: [ pkgs.openssl ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = import ./nix/package.nix {
            inherit pkgs;
            src = self;
          };
        }
      );
      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.python3
            ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.dbus pkgs.playerctl ];
            nativeBuildInputs = nativeInputs pkgs;
            buildInputs = libraryInputs pkgs;
            CARGO_BUILD_JOBS = "4";
          };
        }
      );
    };
}
