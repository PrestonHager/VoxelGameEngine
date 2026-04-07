{
  lib,
  rustPlatform,
  makeWrapper,
  pkg-config,
  vulkan-headers,
  vulkan-loader,
  libGL,
  libxkbcommon,
  wayland,
  xorg,
  fontconfig,
  freetype,
}:

let
  version = "0.1.1";
  runtimeLibs = [
    vulkan-loader
    libGL
    libxkbcommon
    wayland
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    fontconfig
    freetype
  ];
in

rustPlatform.buildRustPackage {
  pname = "vge-editor";
  inherit version;

  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  cargoBuildFlags = [ "-p" "editor" ];
  cargoInstallFlags = [ "-p" "editor" ];
  cargoTestFlags = [ "-p" "editor" ];

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];

  buildInputs = [
    vulkan-headers
    vulkan-loader
  ] ++ runtimeLibs;

  postInstall = ''
    mv "$out/bin/editor" "$out/bin/vge-editor"
    ln -sfn "$out/bin/vge-editor" "$out/bin/editor"

    mkdir -p "$out/share/applications"
    cat > "$out/share/applications/vge-editor.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Version=1.5
Name=VGE Editor
GenericName=Voxel editor
Comment=Voxel game engine - editor and level authoring
Exec=vge-editor %F
Terminal=false
Categories=Development;Graphics;3DGraphics;
Keywords=voxel;editor;game;Vulkan;
EOF
  '';

  postFixup = ''
    wrapProgram "$out/bin/vge-editor" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"
  '';

  meta = with lib; {
    description = "Voxel game engine editor (Vulkan + egui)";
    homepage = "https://github.com/example/voxel-game-engine";
    license = with licenses; [ mit asl20 ];
    mainProgram = "vge-editor";
    platforms = platforms.linux;
  };
}
