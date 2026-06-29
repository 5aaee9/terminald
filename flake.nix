{
  description = "PTY-backed web terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      crane,
      fenix,
      treefmt-nix,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ flake-parts.flakeModules.easyOverlay ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        {
          config,
          lib,
          pkgs,
          system,
          ...
        }:
        let
          rustToolchain = fenix.packages.${system}.stable.toolchain;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.lock
              ./Cargo.toml
              ./crates
              ./frontend/index.html
              ./frontend/package-lock.json
              ./frontend/package.json
              ./frontend/src
              ./frontend/tsconfig.json
              ./frontend/tsconfig.node.json
              ./frontend/vite.config.ts
            ];
          };

          commonArgs = {
            pname = "terminald";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
            inherit src;
            strictDeps = true;

            nativeBuildInputs = [ pkgs.pkg-config ];
          };

          npmArgs = {
            nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
              pkgs.importNpmLock.hooks.linkNodeModulesHook
              pkgs.nodejs
            ];

            npmRoot = "frontend";
            npmDeps = pkgs.importNpmLock.buildNodeModules {
              npmRoot = ./frontend;
              inherit (pkgs) nodejs;
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          terminald = craneLib.buildPackage (
            commonArgs
            // npmArgs
            // {
              inherit cargoArtifacts;

              preBuild = ''
                npm --prefix frontend run build
              '';
            }
          );

          treefmtEval = treefmt-nix.lib.evalModule pkgs {
            projectRootFile = "flake.nix";

            programs.nixfmt = {
              enable = true;
              package = pkgs.nixfmt-rfc-style;
            };

            programs.rustfmt.enable = true;

            programs.prettier = {
              enable = true;
              includes = [ "*.md" ];
            };
          };
        in
        {
          packages = {
            default = terminald;
            terminald = terminald;
          };

          overlayAttrs = {
            inherit (config.packages) terminald;
          };

          checks = {
            formatting = treefmtEval.config.build.check self;
            terminald = terminald;
          };

          formatter = treefmtEval.config.build.wrapper;

          devShells.default = pkgs.mkShell {
            packages = [
              rustToolchain
              config.formatter
              pkgs.cargo-nextest
              pkgs.nodejs
              pkgs.pkg-config
            ];
          };
        };
    };
}
