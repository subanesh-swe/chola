{ self, lib, ... }:
{
  perSystem = { pkgs, ... }:
    let
      # vite runs a strict version check against the esbuild binary. The frontend
      # lockfile pins esbuild 0.25.12, but nixpkgs ships 0.27.2 — the mismatch is
      # what broke earlier builds. Pin a matching esbuild and hand it to vite via
      # ESBUILD_BINARY_PATH (the sandboxed npm postinstall binary can't run).
      esbuild-pinned = pkgs.esbuild.overrideAttrs (_old: {
        version = "0.25.12";
        src = pkgs.fetchFromGitHub {
          owner = "evanw";
          repo = "esbuild";
          rev = "v0.25.12";
          hash = "sha256-iyQP6q/nX4KEo3DZ6H6okgvGiiqatJPPp+mMDOFKu8c=";
        };
      });

      # The built Vite `dist/` as its own artifact. `nix build .#chola-frontend`
      # yields a copyable directory of static assets — deploy it independently of
      # the backend (behind any static server), or run it via the app below.
      chola-frontend-dist = pkgs.buildNpmPackage {
        pname = "chola-frontend";
        version = "0.1.0";

        src = lib.cleanSourceWith {
          src = self + "/frontend";
          # Don't hash node_modules/dist into the source.
          filter = path: _type:
            !(lib.hasInfix "/node_modules" path) && !(lib.hasInfix "/dist" path);
        };

        npmDepsHash = "sha256-C8qfSFPQ/U7wNO7euSkExmRGvinQUc9BZyzzqHK+uoY=";

        ESBUILD_BINARY_PATH = "${esbuild-pinned}/bin/esbuild";

        # rollup 4 ships native code as platform optionalDependencies.
        npmFlags = [ "--include=optional" ];

        # `npm run build` = tsc -b && vite build → dist/
        installPhase = ''
          runHook preInstall
          cp -r dist $out
          runHook postInstall
        '';
      };

      # Serve the built SPA and reverse-proxy /api to the backend, so the browser
      # only ever sees one origin (no CORS). Backend URL and listen port are
      # runtime env vars — the same dist serves any backend.
      caddyfile = pkgs.writeText "Caddyfile" ''
        {
          admin off
          auto_https off
        }
        :{$CHOLA_FRONTEND_PORT:3000} {
          handle /api/* {
            reverse_proxy {$CHOLA_BACKEND_URL:localhost:8080}
          }
          handle {
            root * ${chola-frontend-dist}
            try_files {path} /index.html
            file_server
          }
        }
      '';

      chola-frontend-server = pkgs.writeShellApplication {
        name = "chola-frontend";
        runtimeInputs = [ pkgs.caddy ];
        text = ''
          echo "chola-frontend: serving on :''${CHOLA_FRONTEND_PORT:-3000}, /api -> ''${CHOLA_BACKEND_URL:-localhost:8080}"
          exec caddy run --config ${caddyfile} --adapter caddyfile
        '';
      };
    in
    {
      # nix build .#chola-frontend  -> ./result = static dist/ (copy anywhere)
      packages.chola-frontend = chola-frontend-dist;

      # nix run   .#chola-frontend  -> Caddy serving the dist + /api proxy
      apps.chola-frontend = {
        type = "app";
        program = "${chola-frontend-server}/bin/chola-frontend";
      };
    };
}
