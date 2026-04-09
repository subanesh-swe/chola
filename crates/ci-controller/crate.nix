{ pkgs, ... }:
let
  # Pre-fetch Swagger UI so the build script doesn't need network access
  swaggerUiZip = pkgs.fetchurl {
    url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip";
    sha256 = "sha256-SBJE0IEgl7Efuu73n3HZQrFxYX+cn5UU5jrL4T5xzNw=";
  };
in
{
  crane = {
    args = {
      nativeBuildInputs = [
        pkgs.protobuf
      ];
      # Pre-fetched Swagger UI zip — copied to writable location for the build script.
      # The env var + preBuild apply to BOTH deps and main derivations via crane args.
      SWAGGER_UI_DOWNLOAD_URL = "file:///tmp/swagger-ui.zip";
      preBuild = ''
        cp ${swaggerUiZip} /tmp/swagger-ui.zip
        chmod 644 /tmp/swagger-ui.zip
      '';
    };
  };
}
