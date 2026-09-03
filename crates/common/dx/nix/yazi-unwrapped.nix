{
  rustPlatform,
  version ? "git",
  rev ? "unknown",
  date ? "19700101",
  lib,

  installShellFiles,
  fetchFromGitHub,
  rust-jemalloc-sys,

  imagemagick,
}:
let
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../assets
      ../Cargo.toml
      ../Cargo.lock
      (lib.fileset.fromSource (lib.sources.sourceByRegex ../. [ "^dx-.*" ]))
    ];
  };
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "dx";
  inherit version src;

  cargoLock = {
    lockFile = "${src}/Cargo.lock";
    #outputHashes = {
    #  "mlua-0.10.0" = "sha256-Xg6/jc+UP8tbJJ6x1sbAgt8ZHt051xEBBcjmikQqYlw=";
    #};
  };

  env = {
    dx_GEN_COMPLETIONS = true;
    VERGEN_GIT_SHA = rev;
    VERGEN_BUILD_DATE = builtins.concatStringsSep "-" (builtins.match "(.{4})(.{2})(.{2}).*" date);
  };

  nativeBuildInputs = [
    installShellFiles
    imagemagick
  ];

  buildInputs = [
    rust-jemalloc-sys
  ];

  postInstall = ''
    installShellCompletion --cmd dx \
      --bash ./dx-boot/completions/dx.bash \
      --fish ./dx-boot/completions/dx.fish \
      --zsh  ./dx-boot/completions/_dx

    # Resize logo
    for RES in 16 24 32 48 64 128 256; do
      mkdir -p $out/share/icons/hicolor/"$RES"x"$RES"/apps
      magick assets/logo.png -resize "$RES"x"$RES" $out/share/icons/hicolor/"$RES"x"$RES"/apps/dx.png
    done

    installManPage ${finalAttrs.passthru.srcs.man_src}/dx{.1,-config.5}

    mkdir -p $out/share/applications
    install -m644 assets/dx.desktop $out/share/applications/
  '';

  passthru.srcs = {
    man_src = fetchFromGitHub {
      name = "manpages"; # needed to ensure name is unique
      owner = "dx-rs";
      repo = "manpages";
      rev = "8950e968f4a1ad0b83d5836ec54a070855068dbf";
      hash = "sha256-kEVXejDg4ChFoMNBvKlwdFEyUuTcY2VuK9j0PdafKus=";
    };
  };
  

  meta = {
    description = "Blazing fast terminal file manager written in Rust, based on async I/O";
    homepage = "https://github.com/sxdx/dx";
    license = lib.licenses.mit;
    mainProgram = "dx";
  };
})
