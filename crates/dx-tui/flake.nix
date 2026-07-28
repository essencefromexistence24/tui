{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        toolchain = pkgs.rust-bin.stable.latest.default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        rev = self.shortRev or self.dirtyShortRev or "dirty";
        date = self.lastModifiedDate or self.lastModified or "19700101";
        version =
          (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version
          + "pre${builtins.substring 0 8 date}_${rev}";
      in
      {
        packages = {
          dx-unwrapped = pkgs.callPackage ./nix/dx-unwrapped.nix {
            inherit
              version
              rev
              date
              rustPlatform
              ;
          };
          dx = pkgs.callPackage ./nix/dx.nix {
            inherit (self.packages.${system}) dx-unwrapped;
          };
          default = self.packages.${system}.dx;
        };

        devShells = {
          default = pkgs.callPackage ./nix/shell.nix {
            inherit toolchain;
            inherit (self.packages.${system}) dx dx-unwrapped;
          };
        };

        formatter = pkgs.nixfmt-tree;
      }
    )
    // {
      overlays = {
        default = self.overlays.dx;
        dx = _: prev: {
          inherit (self.packages.${prev.stdenv.system}) dx dx-unwrapped;
        };
      };
    };
}
