{
  description = "Multi-provider Git repository manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    {
      templates.default = {
        description = "repos-manager - ready-to-use workspace";
        path = ./.;
      };
    }
    //
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.stdenv.mkDerivation {
          pname = "repos-manager";
          version = "0.1.0";
          src = ./.;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          installPhase = ''
            mkdir -p $out/bin $out/lib/repos-manager
            cp lib/*.sh $out/lib/repos-manager/
            cp repos-manager.sh $out/bin/repos-manager
            chmod +x $out/bin/repos-manager
            wrapProgram $out/bin/repos-manager \
              --set REPOS_MANAGER_LIB $out/lib/repos-manager \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git pkgs.jq pkgs.gh pkgs.glab pkgs.tea ]}
          '';
        };

        devShells.default = pkgs.mkShell {
          packages = [ self.packages.${system}.default ];

          shellHook = ''
            export REPOS_MANAGER_BASE_DIR="$(pwd)"
          '';
        };
      }
    );
}
