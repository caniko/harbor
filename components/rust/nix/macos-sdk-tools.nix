{
  pkgs,
  rsHarborCli,
  realizeMacosSdkBin,
  validateMacosSdk,
}: {
  inherit realizeMacosSdkBin validateMacosSdk;

  publishMacosSdk = pkgs.writeShellApplication {
    name = "publish-macos-sdk";
    runtimeInputs = [
      rsHarborCli
      realizeMacosSdkBin
      validateMacosSdk
      pkgs.attic-client
    ];
    text = ''
      exec harbor-rs sdk publish-macos "$@"
    '';
  };
}
