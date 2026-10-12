{
  description = "Pinned textamp MVP for NixOS and macOS terminals";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
      nativeInputs = pkgs: [ pkgs.pkg-config ];
      libraryInputs = pkgs: [ pkgs.openssl ]
        ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];
    in {
      packages = forAllSystems (system:
        let pkgs = pkgsFor system;
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "textamp";
            version = "1.0.0-mvp";
            src = pkgs.lib.cleanSourceWith {
              src = self;
              filter = path: type:
                !(builtins.elem (builtins.baseNameOf path) [ "target" "dist" ])
                && pkgs.lib.cleanSourceFilter path type;
            };
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = nativeInputs pkgs;
            nativeCheckInputs = [ pkgs.python3 ];
            buildInputs = libraryInputs pkgs;
            preCheck = ''
              export XDG_CONFIG_HOME="$TMPDIR/textamp-tests/config"
              export XDG_DATA_HOME="$TMPDIR/textamp-tests/data"
              export XDG_CACHE_HOME="$TMPDIR/textamp-tests/cache"
              export XDG_STATE_HOME="$TMPDIR/textamp-tests/state"
              mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
            '';
            meta = {
              description = "Terminal Navidrome music player";
              license = pkgs.lib.licenses.unlicense;
              mainProgram = "textamp";
              platforms = systems;
            };
          };
        });
      devShells = forAllSystems (system:
        let pkgs = pkgsFor system;
        in {
          default = pkgs.mkShell {
            packages = [ pkgs.cargo pkgs.rustc pkgs.rustfmt pkgs.clippy pkgs.python3 ];
            nativeBuildInputs = nativeInputs pkgs;
            buildInputs = libraryInputs pkgs;
            CARGO_BUILD_JOBS = "4";
          };
        });
    };
}
