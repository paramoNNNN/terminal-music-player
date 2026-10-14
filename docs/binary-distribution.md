# Prebuilt Textamp: Nix cache and GitHub Releases

The repository stays private. GitHub Actions builds Linux and macOS on x86_64
and ARM64. There are two installation routes:

- **NixOS / Nix / Home Manager:** keep using the flake package. A private Cachix
  cache supplies the exact package and its runtime dependencies, avoiding Rust
  compilation on your machine once CI has built that revision/system.
- **Direct macOS installation:** download a native GitHub Release archive using
  the authenticated GitHub CLI. The installer verifies its checksums, build
  identity and architecture before replacing the executable atomically.

## One-time GitHub and Cachix setup

1. Create a **private** cache in [Cachix](https://app.cachix.org/). Keep the
   cache's name and public signing key. Confirm the plan supports private caches;
   [Cachix pricing](https://www.cachix.org/pricing) advertises its free tier for
   open-source projects. Create a per-cache read/write token for CI.
2. In this private GitHub repository, open **Settings → Secrets and variables →
   Actions**. Set the repository **variable** `CACHIX_CACHE` to the cache name and
   the repository **secret** `CACHIX_AUTH_TOKEN` to that token. Enter tokens in
   the GitHub UI, never in this repository or in chat.
3. Push the workflows to `origin`. **Nix binary cache** runs on `master` pushes,
   `v*` tags and manual dispatch. It builds/tests the exact existing flake output
   on all four systems, runs the isolated terminal smoke test, and pushes its
   runtime closure. The job is skipped until `CACHIX_CACHE` is configured; a
   missing token then fails with a setup message.
4. On each installing machine, install Cachix (`nix shell nixpkgs#cachix` works)
   and authenticate locally with a **read-only** token for the private cache.
   Use Cachix's documented token setup, then run `cachix use YOUR_CACHE` and
   `cachix doctor`. For a multi-user daemon/NixOS, ensure its root-readable netrc
   and system substituter configuration are set up; user-only configuration
   may not configure the daemon that performs downloads.

[Cachix setup](https://docs.cachix.org/getting-started) describes private-cache
read authentication and `cachix use`. Keep read tokens in a private netrc file,
not in Nix expressions: files copied into the Nix store are readable locally.

## Your Nix configuration: no NUR repository required

You do not need to create `nur.nix` or publish this private project to NUR.
Add Textamp as an input to your existing flake:

```nix
inputs.textamp.url =
  "git+ssh://git@github.com/paramoNNNN/terminal-music-player.git?ref=master";
```

For NixOS, pass inputs to your existing modules:

```nix
outputs = inputs@{ nixpkgs, ... }: {
  nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
    system = "x86_64-linux"; # or aarch64-linux
    specialArgs = { inherit inputs; };
    modules = [ ./configuration.nix ];
  };
};
```

In `configuration.nix` or an imported package-list module:

```nix
{ pkgs, inputs, ... }: {
  environment.systemPackages = [
    inputs.textamp.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];

  # Copy the real name and public signing key from your cache dashboard.
  nix.settings = {
    extra-substituters = [ "https://YOUR_CACHE.cachix.org" ];
    extra-trusted-public-keys = [ "YOUR_CACHE.cachix.org-1:YOUR_PUBLIC_KEY" ];
    # Private cache authentication is stored outside the Nix store.
    netrc-file = "/etc/nix/netrc";
  };
}
```

Use the netrc path actually configured by Cachix if it differs. Provision that
file privately (root-readable for the daemon), not with a token in
`environment.etc.*.text`. The public signing key is safe to commit; the token is
not. Keep your existing substituters/settings too.

For standalone Home Manager, pass `extraSpecialArgs = { inherit inputs; };` to
`home-manager.lib.homeManagerConfiguration`, then add this in `home.nix`:

```nix
{ pkgs, inputs, ... }: {
  home.packages = [
    inputs.textamp.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

When Home Manager is a NixOS module, pass inputs using
`home-manager.extraSpecialArgs = { inherit inputs; };`. Configure the cache for
Nix itself as above. The same flake package also works in nix-darwin's
`environment.systemPackages`.

Then update and rebuild from your configuration checkout:

```sh
nix flake update textamp
nix build --dry-run .#nixosConfigurations.my-host.config.system.build.toplevel
sudo nixos-rebuild switch --flake .#my-host
```

The dry run shows what will be fetched versus built. Nix substitutes only an
exact derivation match: use a pushed, CI-built revision, the same locked inputs
and the default Textamp package. Do not override Textamp's Nixpkgs input or build
flags if you want to reuse these builds. Without a matching accessible cache,
Nix falls back to building from source; it never installs a different revision.

## Publish downloadable releases

**Release binaries** runs for `v*` tags and manual dispatch. Every platform runs
formatting, strict Clippy, all Rust tests, release-tool fixture tests, a release
build with warnings denied, and the isolated terminal smoke test. Each archive
contains `textamp`, `BUILD.json`, the project license and the vendored image
library's license. A SHA-256 sidecar accompanies each archive.

After committing and pushing the workflow, create a version tag on the commit
that you want to ship, for example:

```sh
git tag v1.0.0
git push origin v1.0.0
```

Use a new version tag for subsequent releases. The release publishes only after
all four native jobs succeed; it does not overwrite an existing release. A
manual run builds downloadable workflow artifacts without publishing a release.
Cachix publication is a separate workflow and requires its one-time setup.

Archives are named `textamp-aarch64-darwin.tar.gz`,
`textamp-x86_64-darwin.tar.gz`, `textamp-aarch64-linux.tar.gz` and
`textamp-x86_64-linux.tar.gz`. macOS binaries target macOS 13 or later. Native
Linux archives require system ALSA/OpenSSL 3 libraries (x86_64 builds use Ubuntu
22.04; ARM64 builds use Ubuntu 24.04). NixOS installations use the Nix cache,
which supplies the correct runtime libraries and loader for that system.

## Install/update a macOS release

Install GitHub CLI, authenticate with access to the private repository, then run
from this checkout:

```sh
gh auth login
python3 scripts/install-binary.py
```

The installer resolves the latest published tag once, downloads that exact
platform archive and checksum, verifies them, and atomically replaces
`~/Applications/textamp`. It keeps no old binary backup and leaves the launcher,
configuration, credentials and music alone. This installer performs no source
build. Specify a tag or a different binary path when needed:

```sh
python3 scripts/install-binary.py --tag v1.0.0 --destination "$HOME/.local/bin/textamp"
```

It needs Python 3.8+ and `gh`. It refuses symlink destinations and directs NixOS
users to the flake/cache route. The binary is not Apple-notarized; installation
uses an authenticated CLI download. Checksums detect damaged/mismatched assets;
GitHub authentication supplies access to the private release.

The checkout's `./run` remains the **source-build** launcher. Use the installed
binary (or your separate launcher) to run the prebuilt installation.

## Validation and activation

Local tests can verify workflow syntax and archive/install behavior without
publishing anything. Actual GitHub jobs require the workflows to be pushed;
private Cachix upload requires the cache variable/token. This repository's
setup does not create a Cachix account, alter GitHub settings, create a tag, or
replace a personal installation automatically.
