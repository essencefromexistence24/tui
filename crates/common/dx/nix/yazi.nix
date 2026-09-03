{
  lib,
  formats,
  runCommand,
  makeWrapper,

  runtimeDeps ? (ps: ps),

  # deps
  file,
  dx-unwrapped,

  # default optional deps
  jq,
  poppler-utils,
  _7zz,
  ffmpeg,
  fd,
  ripgrep,
  resvg,
  fzf,
  zoxide,
  imagemagick,
  chafa,

  settings ? { },
  plugins ? { },
  flavors ? { },
  initLua ? null,
}:
let
  inherit (lib)
    concatStringsSep
    concatMapStringsSep
    optionalString
    makeBinPath
    mapAttrsToList
    ;

  defaultDeps = [
    jq
    poppler-utils
    _7zz
    ffmpeg
    fd
    ripgrep
    resvg
    fzf
    zoxide
    imagemagick
    chafa
  ];
  runtimePaths = [ file ] ++ (runtimeDeps defaultDeps);

  settingsFormat = formats.toml { };

  files = [
    "dx"
    "theme"
    "keymap"
  ];

  configHome =
    if (settings == { } && initLua == null && plugins == { } && flavors == { }) then
      null
    else
      runCommand "dx_CONFIG_HOME" { } ''
        mkdir -p $out
        ${concatMapStringsSep "\n" (
          name:
          optionalString (settings ? ${name} && settings.${name} != { }) ''
            ln -s ${settingsFormat.generate "${name}.toml" settings.${name}} $out/${name}.toml
          ''
        ) files}

        mkdir $out/plugins
        ${optionalString (plugins != { }) ''
          ${concatStringsSep "\n" (
            mapAttrsToList (name: value: "ln -s ${value} $out/plugins/${name}") plugins
          )}
        ''}

        mkdir $out/flavors
        ${optionalString (flavors != { }) ''
          ${concatStringsSep "\n" (
            mapAttrsToList (name: value: "ln -s ${value} $out/flavors/${name}") flavors
          )}
        ''}


        ${optionalString (initLua != null) "ln -s ${initLua} $out/init.lua"}
      '';
in
runCommand dx-unwrapped.name
  {
    inherit (dx-unwrapped) pname version meta;

    nativeBuildInputs = [ makeWrapper ];

    passthru.runtimePaths = runtimePaths;
  }
  ''
    mkdir -p $out/bin
    ln -s ${dx-unwrapped}/share $out/share
    ln -s ${dx-unwrapped}/bin/ya $out/bin/ya
    makeWrapper ${dx-unwrapped}/bin/dx $out/bin/dx \
      --prefix PATH : "${makeBinPath runtimePaths}" \
      ${optionalString (configHome != null) "--set dx_CONFIG_HOME ${configHome}"}
  ''
