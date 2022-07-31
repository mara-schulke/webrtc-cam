with import <nixpkgs> {};

stdenv.mkDerivation {
  name = "webrtc-cam";

  buildInputs = with pkgs; [
    openssl
    libnice
    glib
    pkgconfig
    gcc
    gst_all_1.gstreamer
    gst_all_1.gst-rtsp-server
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good 
    gst_all_1.gst-plugins-bad 
    gst_all_1.gst-plugins-ugly 
    clang libftdi1
    opencv
  ];

  shellHook = with pkgs; ''
    export LIBCLANG_PATH="${llvmPackages.libclang.lib}/lib";
  '';
}
