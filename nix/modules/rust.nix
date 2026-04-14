{ inputs, self, ... }:
{
  imports = [
    inputs.rust-flake.flakeModules.default
    inputs.rust-flake.flakeModules.nixpkgs
  ];
  perSystem = { config, self', pkgs, lib, ... }: {
    rust-project = {
      # See /crates/*/crate.nix for the crate-specific Nix configuration
      crateNixFile = "crate.nix";
      src = lib.cleanSourceWith {
        src = self; # The original, unfiltered source
        filter = path: type:
          (config.rust-project.crateNixFile != null && lib.hasSuffix "/${config.rust-project.crateNixFile}" path) ||
          (lib.hasSuffix ".proto" path) ||
          (lib.hasSuffix ".sql" path) ||
          # Default filter from crane (allow .rs files)
          (config.rust-project.crane-lib.filterCargoSources path type)
        ;
      };
    };
    packages.default = self'.packages.chola-controller;
  };
}
