# Prebuilt Textamp and public Nix distribution

This fork is fully modified by AI. The original application is [bergmayer/textamp](https://github.com/bergmayer/textamp), by John Bergmayer, under the Unlicense.

The supported systems are Apple Silicon macOS (`aarch64-darwin`), x86_64 Linux and ARM64 Linux. See [public installation and NUR registration](../nix/README.md) for the package entrypoint and registration instructions.

## Public Cachix cache

The cache is `https://terminal-music-player.cachix.org`. Reading it requires no token. Configure the following in NixOS or nix-darwin:

```nix
nix.settings = {
  extra-substituters = [ "https://terminal-music-player.cachix.org" ];
  extra-trusted-public-keys = [
    "terminal-music-player.cachix.org-1:3lKj0+0+0WnRLNi9QXalpYPnS8BVJK1lSgfOeQJW6+k="
  ];
};
```

These settings take effect after activation. For the first installation, activate the cache configuration first or supply these options to the privileged build command. Nix only substitutes an exact matching store path: keep the flake's locked dependencies and use a CI-built revision. NUR evaluates with its supplied Nixpkgs, which may produce a different derivation and require compilation.

## GitHub Actions setup

In the public fork's Settings → Secrets and variables → Actions:

1. Set repository variable `CACHIX_CACHE` to `terminal-music-player`.
2. Set secret `CACHIX_AUTH_TOKEN` to a Cachix token with write access to that cache. Enter it locally or in GitHub; never commit it.
3. Enable Actions for the fork. The Nix binary cache workflow builds/tests the exact flake output and pushes the runtime closure on master pushes, version tags or manual dispatch.

A newly created fork does not inherit the standalone repository's secrets, variables or workflow runs. Keep that original repository available until the new fork's CI is configured.

## Public flake installation

After publication:

```sh
nix run github:paramoNNNN/terminal-music-player
```

The flake exports `packages.<system>.default`. Both the flake and NUR entrypoint use `nix/package.nix`.

## Release archives

The Release binaries workflow runs on version tags and manual dispatch. It builds/tests supported native platforms, packages `textamp` with `BUILD.json` and checksums, and publishes tagged releases. A manual run only uploads workflow artifacts.

After a release is published, the installer can use GitHub CLI to download and verify it:

```sh
python3 scripts/install-binary.py
```

The macOS destination is `~/Applications/textamp`. The separate launcher and personal configuration are preserved. macOS archives are not Apple-notarized. Linux archives require the matching system libraries; NixOS users should use the Nix package. The checkout's `./run` launcher builds from source.

The optional AirPlay helper is distributed separately; see the main README for its source and license.
