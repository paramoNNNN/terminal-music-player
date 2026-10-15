# Public installation and NUR registration

This fork is fully modified by AI. Original application: [bergmayer/textamp](https://github.com/bergmayer/textamp), by John Bergmayer (Unlicense).

## Direct flake installation

Once the repository is public:

```sh
nix run github:paramoNNNN/terminal-music-player
```

Configure the public cache in NixOS or nix-darwin:

```nix
nix.settings = {
  extra-substituters = [ "https://terminal-music-player.cachix.org" ];
  extra-trusted-public-keys = [
    "terminal-music-player.cachix.org-1:3lKj0+0+0WnRLNi9QXalpYPnS8BVJK1lSgfOeQJW6+k="
  ];
};
```

Activate the cache settings before the first installation, or pass them as Nix command options with the privileges required by your daemon. The public cache requires no token. Only exact matching builds are substituted. Keep the flake's locked Nixpkgs if reusing CI builds.

## Register with NUR

The root `default.nix` accepts NUR's `pkgs` argument and exports `textamp`. Both the flake and NUR use `nix/package.nix`. NUR's Nixpkgs can differ from the flake's pin, so NUR installations may build from source even when the flake output is cached.

After publishing these files and making the repository public, submit a pull request to [nix-community/NUR](https://github.com/nix-community/NUR) adding this entry inside `repos.json`'s `repos` object:

```json
"paramoNNNN": {
  "url": "https://github.com/paramoNNNN/terminal-music-player"
}
```

After the registration is merged and indexed, users with the NUR overlay can install:

```nix
home.packages = [ pkgs.nur.repos.paramoNNNN.textamp ];
```

See the [NUR registration instructions](https://nur.nix-community.org/documentation/#how-to-add-your-own-repository).
