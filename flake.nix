{
  description = "A Rust project";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";

    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crate2nix = {
      url = "github:nix-community/crate2nix";
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
                cargo = config.devenv.shells.default.languages.rust.toolchainPackage;
                clippy = config.devenv.shells.default.languages.rust.toolchainPackage;
              };
            };

            languages.rust = {
              enable = true;
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

          packages.default = config.devenv.shells.default.languages.rust.import ./. { };

        };
    };
}
