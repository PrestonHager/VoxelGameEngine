/**
  Overlay: adds `vge-editor` to nixpkgs.

  **NixOS** (`configuration.nix`):

  ```nix
  { pkgs, ... }: {
    nixpkgs.overlays = [
      (import /path/to/VoxelGameEngine/nix/overlay.nix)
    ];
    environment.systemPackages = [ pkgs.vge-editor ];
  }
  ```

  **Flake input** (this repo as `vge`):

  ```nix
  nixpkgs.overlays = [ vge.overlays.default ];
  environment.systemPackages = [ pkgs.vge-editor ];
  ```

  **Ad-hoc shell** (from repo root):

  ```bash
  nix-shell -E 'with import <nixpkgs> { overlays = [ (import ./nix/overlay.nix) ]; }; mkShell { packages = [ vge-editor ]; }'
  ```

  Prefer `nix develop` or `nix shell .#vge-editor` when using the bundled `flake.nix`.
*/
final: prev: {
  vge-editor = final.callPackage ./package.nix { };
}
