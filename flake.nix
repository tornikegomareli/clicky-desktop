{
  description = "Clicky Desktop - AI companion with voice input and cursor overlay";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        runtimeLibs = with pkgs; [
          alsa-lib
          pipewire
          gtk3
          openssl
          libGL
          libgbm
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxrandr
          libxi
          libxinerama
          libxcb
          systemd
          xdotool
        ];

        runtimeBins = with pkgs; [
          grim
        ];

        buildInputs = runtimeLibs;

        nativeBuildInputs = with pkgs; [
          pkg-config
          cmake
          clang
          makeWrapper
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "clicky-desktop";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit buildInputs nativeBuildInputs;

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          postInstall = ''
            wrapProgram $out/bin/clicky-desktop \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeBins} \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
          '';

          meta = with pkgs.lib; {
            description = "AI companion that lives in system tray, listens to voice and points at things";
            homepage = "https://github.com/tornikegomareli/clicky-desktop";
            license = licenses.mit;
            platforms = platforms.linux;
            mainProgram = "clicky-desktop";
          };
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };

        devShells.default = pkgs.mkShell {
          inherit buildInputs;
          nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [ cargo rustc ]);
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };
      }
    );
}
