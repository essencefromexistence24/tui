#!/usr/bin/env node

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

type ColorMode = Record<string, string | undefined>;

type SourceTheme = {
  name: string;
  title: string;
  description: string;
  cssVars: {
    dark?: ColorMode;
    light?: ColorMode;
  };
};

type ThemeCatalog = {
  items: SourceTheme[];
};

type TuiColorMode = Record<string, string>;

type TuiTheme = {
  name: string;
  title: string;
  description: string;
  dark: TuiColorMode;
  light: TuiColorMode;
};

type RgbColor = {
  r: number;
  g: number;
  b: number;
};

type RgbTheme = {
  name: string;
  title: string;
  description: string;
  dark: Record<string, RgbColor>;
  light: Record<string, RgbColor>;
};

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));

function oklchToRgb(oklchStr: string): [number, number, number] {
  const match = oklchStr.match(/oklch\(([\d.]+)\s+([\d.]+)\s+([\d.]+)\)/);
  if (!match) {
    console.warn(`Could not parse OKLCH: ${oklchStr}`);
    return [128, 128, 128];
  }

  const [, lightness, chroma, hue] = match.map(Number);
  
  const l = lightness * 100;
  const a = chroma * 100 * Math.cos((hue * Math.PI) / 180);
  const b = chroma * 100 * Math.sin((hue * Math.PI) / 180);
  
  // Lab to XYZ conversion
  let fy = (l + 16) / 116;
  let fx = a / 500 + fy;
  let fz = fy - b / 200;
  
  const delta = 6 / 29;
  const deltaSquared = delta * delta;
  const deltaCubed = delta * delta * delta;
  
  const xr = fx > delta ? fx * fx * fx : 3 * deltaSquared * (fx - 4 / 29);
  const yr = fy > delta ? fy * fy * fy : 3 * deltaSquared * (fy - 4 / 29);
  const zr = fz > delta ? fz * fz * fz : 3 * deltaSquared * (fz - 4 / 29);
  
  // D65 illuminant
  const x = xr * 95.047;
  const y = yr * 100.000;
  const z = zr * 108.883;
  
  // XYZ to RGB (sRGB color space)
  let r = x *  3.2406 + y * -1.5372 + z * -0.4986;
  let g = x * -0.9689 + y *  1.8758 + z *  0.0415;
  let bl = x *  0.0557 + y * -0.2040 + z *  1.0570;
  
  // Apply gamma correction (sRGB)
  const gammaCorrect = (c: number) => {
    c = c / 100;
    return c > 0.0031308 ? 1.055 * Math.pow(c, 1 / 2.4) - 0.055 : 12.92 * c;
  };
  
  r = gammaCorrect(r);
  g = gammaCorrect(g);
  bl = gammaCorrect(bl);
  
  // Clamp and convert to 0-255
  r = Math.max(0, Math.min(255, Math.round(r * 255)));
  g = Math.max(0, Math.min(255, Math.round(g * 255)));
  bl = Math.max(0, Math.min(255, Math.round(bl * 255)));
  
  return [r, g, bl];
}

function extractTuiColors(theme: SourceTheme): TuiTheme {
  const darkColors = theme.cssVars.dark || {};
  const lightColors = theme.cssVars.light || {};
  
  const extractMode = (colors: ColorMode): TuiColorMode => ({
    background: colors.background || 'oklch(0 0 0)',
    foreground: colors.foreground || 'oklch(1 0 0)',
    card: colors.card || colors.background || 'oklch(0 0 0)',
    card_foreground: colors['card-foreground'] || colors.foreground || 'oklch(1 0 0)',
    primary: colors.primary || 'oklch(0.5 0.2 200)',
    primary_foreground: colors['primary-foreground'] || 'oklch(1 0 0)',
    secondary: colors.secondary || 'oklch(0.3 0 0)',
    secondary_foreground: colors['secondary-foreground'] || 'oklch(1 0 0)',
    muted: colors.muted || 'oklch(0.3 0 0)',
    muted_foreground: colors['muted-foreground'] || 'oklch(0.7 0 0)',
    accent: colors.accent || colors.primary || 'oklch(0.5 0.2 200)',
    accent_foreground: colors['accent-foreground'] || 'oklch(1 0 0)',
    destructive: colors.destructive || 'oklch(0.6 0.2 25)',
    destructive_foreground: colors['destructive-foreground'] || 'oklch(1 0 0)',
    border: colors.border || 'oklch(0.3 0 0)',
    input: colors.input || colors.border || 'oklch(0.3 0 0)',
    ring: colors.ring || colors.primary || 'oklch(0.5 0.2 200)',
  });
  
  return {
    name: theme.name,
    title: theme.title,
    description: theme.description,
    dark: extractMode(darkColors),
    light: extractMode(lightColors),
  };
}

function convertThemeToRgb(theme: TuiTheme): RgbTheme {
  const convertMode = (mode: TuiColorMode): Record<string, RgbColor> => {
    const rgb: Record<string, RgbColor> = {};
    for (const [key, value] of Object.entries(mode)) {
      const [r, g, b] = oklchToRgb(value);
      rgb[key] = { r, g, b };
    }
    return rgb;
  };
  
  return {
    name: theme.name,
    title: theme.title,
    description: theme.description,
    dark: convertMode(theme.dark),
    light: convertMode(theme.light),
  };
}

function main() {
  const themeJsonPath = path.join(scriptDirectory, "..", "theme.json");
  const outputPath = path.join(scriptDirectory, "..", "themes.json");
  
  console.log("Reading theme.json...");
  const themeData = JSON.parse(fs.readFileSync(themeJsonPath, "utf8")) as ThemeCatalog;
  
  console.log(`Found ${themeData.items.length} themes`);
  
  const tuiThemes = themeData.items.map(theme => {
    console.log(`Processing: ${theme.title}`);
    const extracted = extractTuiColors(theme);
    return convertThemeToRgb(extracted);
  });
  
  const output = {
    version: "1.0.0",
    themes: tuiThemes,
  };
  
  console.log(`Writing ${tuiThemes.length} themes to themes.json...`);
  fs.writeFileSync(outputPath, JSON.stringify(output, null, 2), "utf8");
  
  console.log("Done. Created themes.json");
  console.log(`  Themes: ${tuiThemes.length}`);
  console.log(`  Output: ${outputPath}`);
  
  // Show a sample of the first theme's colors for verification
  if (tuiThemes.length > 0) {
    console.log("\nSample colors from first theme:");
    const firstTheme = tuiThemes[0];
    console.log(`  Background: rgb(${firstTheme.dark.background.r}, ${firstTheme.dark.background.g}, ${firstTheme.dark.background.b})`);
    console.log(`  Foreground: rgb(${firstTheme.dark.foreground.r}, ${firstTheme.dark.foreground.g}, ${firstTheme.dark.foreground.b})`);
    console.log(`  Primary: rgb(${firstTheme.dark.primary.r}, ${firstTheme.dark.primary.g}, ${firstTheme.dark.primary.b})`);
  }
}

main();
