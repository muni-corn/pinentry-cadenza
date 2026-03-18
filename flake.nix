{
  description = "A Rust project";

  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";

    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

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
          pname = "pinentry-cadenza";
          buildInputs = with pkgs; [
            gtk4
            gtk4-layer-shell
            libgcc
            libxkbcommon
            wayland
          ];
          nativeBuildInputs = with pkgs; [
            autoPatchelfHook
            pkg-config
          ];
          libraryPath = lib.makeLibraryPath buildInputs;

          toolchain = config.devenv.shells.default.languages.rust.toolchainPackage;
        in
        {
          # rust setup
          devenv.shells.default = {
            git-hooks.hooks.clippy = {
              enable = true;
              packageOverrides = {
                cargo = toolchain;
                clippy = toolchain;
              };
            };

            languages.rust = {
              enable = true;
              mold.enable = true;

              # needed for dynamic linking at runtime
              rustflags = "-C link-args=-Wl,-fuse-ld=mold,-rpath,${libraryPath}";
            };

            packages =
              with pkgs;
              [
                bacon
                cargo-outdated
                cargo-tarpaulin
                pkg-config

                # for testing fallback
                pinentry-curses
                pinentry-tty
              ]
              ++ buildInputs
              ++ nativeBuildInputs;

            scripts.tarp.exec = ''cargo tarpaulin --engine llvm "$@"'';
          };

          packages.default =
            let
              args = {
                crateOverrides = pkgs.defaultCrateOverrides // {
                  ${pname} = attrs: {
                    inherit buildInputs nativeBuildInputs;
                    runtimeDependencies = buildInputs;
                  };
                };
              };
            in
            config.devenv.shells.default.languages.rust.import ./. args;
        };
    };
}
