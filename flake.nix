{
  description = "fin";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      createDevShell =
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            name = "fin";
            nativeBuildInputs = with pkgs; [
              git
              python3
              rustup
            ];
            buildInputs = with pkgs; [
              libiconv
            ];
          };
        };
    in
    {
      devShells = nixpkgs.lib.genAttrs systems createDevShell;
    };
}
