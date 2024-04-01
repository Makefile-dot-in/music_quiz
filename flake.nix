{
  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url = "github:nix-community/fenix/monthly";
    fenix.inputs = { nixpkgs.follows = "nixpkgs"; };

  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs = { self, nixpkgs, devenv, systems, fenix, ... } @ inputs:
    let
      forEachSystem = nixpkgs.lib.genAttrs (import systems);
    in
    {
      packages = forEachSystem (system: {
        devenv-up = self.devShells.${system}.default.config.procfileScript;
      });

      devShells = forEachSystem
        (system:
          let
            pkgs = nixpkgs.legacyPackages.${system};
            fenixpkgs = fenix.packages.${system};
          in
          {
            default = devenv.lib.mkShell {
              inherit inputs pkgs;
              modules = [
                {
                  # https://devenv.sh/reference/options/A

                  env.DATABASE_URL = "postgres://localhost:12345/music_quiz";

                  languages.rust = {
                    enable = true;
                    channel = "nightly";
                    components = [ "rustc" "cargo" "clippy" "rustfmt" ];
                  };
                  #
                  packages = [ pkgs.sqlx-cli fenixpkgs.rust-analyzer ];

                  services.postgres = {
                    enable = true;
                    initialDatabases = [ { name = "music_quiz"; } ];
                    listen_addresses = "127.0.0.1";
                    port = 12345;
                    initialScript = ''
                    CREATE ROLE postgres SUPERUSER;
                    '';
                  };
                }
              ];
            };
          });
    };
}
