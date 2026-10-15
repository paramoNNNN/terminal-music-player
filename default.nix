{ pkgs }: {
  textamp = import ./nix/package.nix { inherit pkgs; };
}
