{
  mkShell,
  dx,
  toolchain,
  nodePackages,
  dx-unwrapped,
}:

mkShell {
  packages = dx.passthru.runtimePaths ++ [
    (toolchain.override {
      extensions = [
        "rust-src"
        "rustfmt"
        "rust-analyzer"
        "clippy"
      ];
    })
    nodePackages.cspell
  ];

  inputsFrom = [ dx-unwrapped ];

  env.RUST_BACKTRACE = "1";
}
