# Generates xai-grok-pager-render/src/theme/dx_themes.rs from themes.json.
#
# Maps each DX theme's dark CSS-variant palette (17 color slots) onto the
# pager's ~60-field semantic Theme struct. Run from the repo root:
#
#   pwsh crates/common/dx/scripts/generate-pager-dx-themes.ps1

$ErrorActionPreference = "Stop"

$root = git rev-parse --show-toplevel
$themesJson = Join-Path $root "crates/common/dx/themes.json"
$output = Join-Path $root "crates/codegen/xai-grok-pager-render/src/theme/dx_themes.rs"

$data = Get-Content -Raw $themesJson | ConvertFrom-Json

function Darken([int]$r, [int]$g, [int]$b, [double]$factor) {
    return @{
        r = [math]::Round($r * $factor)
        g = [math]::Round($g * $factor)
        b = [math]::Round($b * $factor)
    }
}

function Cv([hashtable]$c) {
    return "rgb($($c.r), $($c.g), $($c.b))"
}

# Convert a JSON color object ({r,g,b}) into a hashtable.
function RgbOf($c) {
    return @{ r = [int]$c.r; g = [int]$c.g; b = [int]$c.b }
}

# Map a DX dark palette onto pager Theme fields.
function Map-PagerTheme($d) {
    $bg = RgbOf $d.background
    $fg = RgbOf $d.foreground
    $card = RgbOf $d.card
    $cardFg = RgbOf $d.card_foreground
    $primary = RgbOf $d.primary
    $primaryFg = RgbOf $d.primary_foreground
    $secondary = RgbOf $d.secondary
    $secondaryFg = RgbOf $d.secondary_foreground
    $muted = RgbOf $d.muted
    $mutedFg = RgbOf $d.muted_foreground
    $accent = RgbOf $d.accent
    $accentFg = RgbOf $d.accent_foreground
    $destructive = RgbOf $d.destructive
    $destructiveFg = RgbOf $d.destructive_foreground
    $border = RgbOf $d.border
    $input = RgbOf $d.input
    $ring = RgbOf $d.ring

    $diffDelBg = Darken $destructive.r $destructive.g $destructive.b 0.2
    $diffInsBg = Darken $accent.r $accent.g $accent.b 0.2

    return @"
            bg_base: $(Cv $bg),
            bg_light: $(Cv $card),
            bg_dark: $(Cv $secondary),
            bg_highlight: $(Cv $card),
            bg_hover: $(Cv $muted),
            bg_terminal: $(Cv $bg),
            accent_user: $(Cv $primary),
            accent_assistant: $(Cv $accent),
            accent_thinking: $(Cv $mutedFg),
            accent_tool: $(Cv $secondaryFg),
            accent_system: $(Cv $primary),
            accent_error: $(Cv $destructive),
            accent_success: $(Cv $accent),
            accent_running: $(Cv $accent),
            accent_skill: $(Cv $accent),
            text_primary: $(Cv $fg),
            text_secondary: $(Cv $mutedFg),
            gray_dim: $(Cv $border),
            gray: $(Cv $mutedFg),
            gray_bright: $(Cv $cardFg),
            command: $(Cv $accent),
            path: $(Cv $ring),
            running: $(Cv $accent),
            warning: $(Cv $destructive),
            fuzzy_accent: $(Cv $primary),
            accent_plan: $(Cv $accent),
            accent_verify: $(Cv $accent),
            accent_feedback: $(Cv $accent),
            accent_remember: $(Cv $accent),
            selection_border: $(Cv $ring),
            hover_border: $(Cv $border),
            prompt_border: $(Cv $border),
            prompt_border_active: $(Cv $primary),
            accent_model: $(Cv $accent),
            scrollbar_bg: $(Cv $muted),
            scrollbar_fg: $(Cv $border),
            diff_delete_bg: $(Cv $diffDelBg),
            diff_delete_fg: $(Cv $destructive),
            diff_insert_bg: $(Cv $diffInsBg),
            diff_insert_fg: $(Cv $accent),
            diff_equal_fg: $(Cv $mutedFg),
            diff_gutter_fg: $(Cv $border),
            bg_visual: $(Cv $card),
            paste_bg: $(Cv $input),
            paste_fg: $(Cv $mutedFg),
            paste_dim: $(Cv $border),
            md_heading_h1: $(Cv $accent),
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: $(Cv $accent),
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: $(Cv $accent),
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: $(Cv $accent),
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: $(Cv $accent),
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: $(Cv $accent),
            md_heading_h6_mod: Modifier::BOLD,
            md_code: $(Cv $accent),
            md_task_checked: $(Cv $accent),
            md_task_unchecked: $(Cv $mutedFg),
            md_muted: $(Cv $mutedFg),
            md_code_bg: $(Cv $input),
            md_text: $(Cv $fg),
            link_fg: $(Cv $primary),
"@
}

function SafeIdent([string]$name) {
    return $name -replace "-", "_"
}

$body = @()
$body += "//! DX themes auto-generated from dx-tui themes.json (dark variant)."
$body += "//! Regenerate with: pwsh crates/common/dx/scripts/generate-pager-dx-themes.ps1"
$body += "use ratatui::style::{Color, Modifier};"
$body += "use super::tokyonight::Theme;"
$body += ""
$body += "/// Helper for concise const Color::Rgb definitions."
$body += "const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }"
$body += ""
$body += "/// All DX theme names, in catalog order."
$body += "pub const DX_THEME_NAMES: &[&str] = &["
foreach ($t in $data.themes) {
    $body += "    `"$($t.name)`","
}
$body += "];"
$body += ""
$body += "/// Display titles for each DX theme, parallel to `DX_THEME_NAMES`."
$body += "pub const DX_THEME_TITLES: &[&str] = &["
foreach ($t in $data.themes) {
    $body += "    `"$($t.title)`","
}
$body += "];"
$body += ""
$body += "/// Display title for a DX theme name."
$body += "pub fn dx_theme_title(name: &str) -> Option<&'static str> {"
$body += "    DX_THEME_NAMES.iter().position(|n| *n == name).map(|i| DX_THEME_TITLES[i])"
$body += "}"
$body += ""
$body += "/// Build the pager Theme for a DX theme name (dark variant)."
$body += "pub fn dx_theme(name: &str) -> Option<Theme> {"
$body += "    match name {"
foreach ($t in $data.themes) {
    $ident = SafeIdent $t.name
    $body += "        `"$($t.name)`" => Some(Theme::$ident()),"
}
$body += "        _ => None,"
$body += "    }"
$body += "}"
$body += ""

foreach ($t in $data.themes) {
    $ident = SafeIdent $t.name
    $title = $t.title -replace '"', '\"'
    $mapped = Map-PagerTheme $t.dark
    $body += "/// DX theme: `"$($t.title)`" (``$($t.name)``)."
    $body += "impl Theme {"
    $body += "    pub const fn $ident() -> Self {"
    $body += "        Self {"
    $body += $mapped.TrimEnd("`n")
    $body += "        }"
    $body += "    }"
    $body += "}"
    $body += ""
}

$content = $body -join "`n"
Set-Content -Path $output -Value $content -NoNewline -Encoding utf8
Write-Host "Wrote $($data.themes.Count) themes to $output"

