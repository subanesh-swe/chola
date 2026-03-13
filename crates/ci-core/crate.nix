{ pkgs, ... }: {
  crane = {
    args = {
      nativeBuildInputs = [
        pkgs.protobuf
      ];
    };
  };
}
