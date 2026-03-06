{
  description = "A Rust project";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    musicaloft-style = {
      url = "github:musicaloft/musicaloft-style";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-flake = {
      url = "github:juspay/rust-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      imports = [
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs

        # sets up code formatting and commit linting
        inputs.musicaloft-style.flakeModule
      ];

      perSystem =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          pname = "pinentry-cadenze";

          buildInputs = with pkgs; [
            libx11
            libxcb
            libxcursor
            libxi
            libxkbcommon
            libxrandr
            vulkan-loader
            wayland
            wayland
          ];
          nativeBuildInputs = [ ];
        in
        {
          # rust setup
          devenv.shells.default = {
            env.LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${lib.makeLibraryPath buildInputs}";

            git-hooks.hooks.clippy = {
              enable = true;
              packageOverrides = {
                cargo = config.rust-project.toolchain;
                clippy = config.rust-project.toolchain;
              };
            };

            languages.rust = {
              enable = true;
              channel = "nightly";
              mold.enable = true;
            };

            packages =
              with pkgs;
              [
                bacon
                cargo-outdated
                cargo-tarpaulin
              ]
              ++ buildInputs
              ++ nativeBuildInputs;

            scripts.tarp.exec = ''cargo tarpaulin --engine llvm "$@"'';
          };

          # rust build settings
          rust-project = {
            # use the same rust toolchain from the dev shell for consistency
            toolchain = config.devenv.shells.default.languages.rust.toolchainPackage;

            # specify dependencies
            defaults.perCrate.crane.args = {
              inherit nativeBuildInputs buildInputs;
            };
          };

          packages.default = config.rust-project.crates.${pname}.crane.outputs.packages.${pname};
        };
    };
}
