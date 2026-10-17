{
  pkgs,
  src ? ../.,
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "textamp";
  version = "1.0.0-mvp";
  src = pkgs.lib.cleanSourceWith {
    inherit src;
    filter =
      path: type:
      !(builtins.elem (builtins.baseNameOf path) [
        "target"
        "dist"
      ])
      && pkgs.lib.cleanSourceFilter path type;
  };
  cargoLock.lockFile = src + "/Cargo.lock";
  # Test the full suite without applying release LTO to every test binary.
  checkType = "debug";
  env = {
    CARGO_INCREMENTAL = "0";
    CARGO_PROFILE_DEV_DEBUG = "0";
    CARGO_PROFILE_TEST_DEBUG = "0";
    CARGO_PROFILE_DEV_STRIP = "symbols";
    CARGO_PROFILE_TEST_STRIP = "symbols";
  };
  nativeBuildInputs = [ pkgs.pkg-config ];
  nativeCheckInputs = [ pkgs.python3 ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.dbus pkgs.playerctl ];
  buildInputs = [
    pkgs.openssl
  ]
  ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];
  preCheck = ''
    export XDG_CONFIG_HOME="$TMPDIR/textamp-tests/config"
    export XDG_DATA_HOME="$TMPDIR/textamp-tests/data"
    export XDG_CACHE_HOME="$TMPDIR/textamp-tests/cache"
    export XDG_STATE_HOME="$TMPDIR/textamp-tests/state"
    mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
  '';
  meta = {
    description = "Terminal music player for Navidrome, Subsonic, WebDAV and local files";
    homepage = "https://github.com/paramoNNNN/terminal-music-player";
    license = pkgs.lib.licenses.unlicense;
    mainProgram = "textamp";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
  };
}
