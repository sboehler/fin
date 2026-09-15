{
  description = "fin";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1";
  };

  outputs =
    { self, nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems =
        f: lib.genAttrs systems (system: f (import nixpkgs { inherit system; }));
    in
    {
      packages = forAllSystems (pkgs: rec {
        default = fin;
        fin = pkgs.rustPlatform.buildRustPackage {
          pname = "fin";
          version = (lib.importTOML ./Cargo.toml).package.version;

          # Only the inputs cargo actually needs, so that edits to the README
          # or the flake itself do not trigger a rebuild. `testdata/public`
          # feeds the golden tests in `checkPhase`; `testdata/private` is a
          # submodule and is not part of the flake source, which the golden
          # test tolerates by skipping missing roots.
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              ./tests
              ./testdata/public
            ];
          };

          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "Plain-text accounting tool";
            homepage = "https://github.com/sboehler/fin";
            license = lib.licenses.asl20;
            mainProgram = "fin";
          };
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          name = "fin";
          nativeBuildInputs = with pkgs; [
            git
            python3
            rustup
          ];
        };
      });
    };
}
