# DX TUI Default Configuration

This directory contains the default configuration files embedded in the DX TUI file-browser engine:

- [`dx-default.toml`][dx-default]: General configuration
- [`keymap-default.toml`][keymap-default]: Keybindings configuration
- [`theme-dark.toml`][theme-dark]: Dark color scheme (loaded when your terminal is in dark mode)
- [`theme-light.toml`][theme-light]: Light color scheme (loaded when your terminal is in light mode)

These files are already included with DX TUI, so you do not need to manually download or copy them to your DX configuration directory.

However, if you want to customize certain configurations:

- Create a `dx.toml` in your config directory to override certain settings in [`dx-default.toml`][dx-default], so either:
  - `~/.config/dx/dx.toml` on Unix-like systems
  - `%AppData%\dx\config\dx.toml` on Windows
- Create a `keymap.toml` in your config directory to override certain settings in [`keymap-default.toml`][keymap-default], so either:
  - `~/.config/dx/keymap.toml` on Unix-like systems
  - `%AppData%\dx\config\keymap.toml` on Windows
- Create a `theme.toml` in your config directory to override certain settings in [`theme-light.toml`][theme-light] and [`theme-dark.toml`][theme-dark], so either:
  - `~/.config/dx/theme.toml` on Unix-like systems
  - `%AppData%\dx\config\theme.toml` on Windows

For the user's `theme.toml` file, you can only apply the same color scheme to both the light and dark themes.

If you want more granular control over colors, specify two different flavors for light and dark modes under the `[flavor]` section of your `theme.toml`, and override them in your respective flavor instead.

[dx-default]: dx-default.toml
[keymap-default]: keymap-default.toml
[theme-dark]: theme-dark.toml
[theme-light]: theme-light.toml
