# What the window needs while it runs, shared by the package and the development shell.
#
# NixOS has no /usr/lib: the window opens its graphics, keyboard and window-system libraries at run time
# (winit and glutin do), so their folders go on LD_LIBRARY_PATH. The tools it starts (parec and pactl to hear the
# call, grim and friends for a screenshot, xdg-open for a link) are put at the end of PATH: the ones the system
# has win, these are the fallback.
{
  lib,
  wayland,
  libxkbcommon,
  libGL,
  libglvnd,
  vulkan-loader,
  # the X libraries moved out of `xorg` to the top level in nixpkgs: either place works here
  xorg ? null,
  libx11 ? null,
  libxcursor ? null,
  libxi ? null,
  libxrandr ? null,
  libxcb ? null,
  pulseaudio,
  grim,
  maim,
  scrot,
  xdg-utils,
}:
let
  pick = top: old: if top != null then top else xorg.${old};
  libraries = [
    wayland
    libxkbcommon
    libGL
    libglvnd
    vulkan-loader
    (pick libx11 "libX11")
    (pick libxcursor "libXcursor")
    (pick libxi "libXi")
    (pick libxrandr "libXrandr")
    (pick libxcb "libxcb")
  ];
  tools = [
    pulseaudio
    grim
    maim
    scrot
    xdg-utils
  ];
in
{
  inherit libraries tools;
  # /run/opengl-driver/lib holds the graphics driver of the machine (hardware.graphics.enable on NixOS)
  libraryPath = "/run/opengl-driver/lib:${lib.makeLibraryPath libraries}";
  binPath = lib.makeBinPath tools;
}
