#!/usr/bin/env node

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

type Theme = {
  name: string;
  type: string;
  title: string;
  description: string;
  css?: Record<string, unknown>;
  cssVars: Record<string, Record<string, string>>;
};

type ThemeCatalog = {
  items: Theme[];
};

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));

const dxTheme: Theme = {
  "name": "dx",
  "type": "registry:style",
  "title": "DX",
  "description": "The default DX theme with green accents.",
  "css": {
    "@layer base": {
      "body": {
        "letter-spacing": "var(--tracking-normal)"
      }
    }
  },
  "cssVars": {
    "theme": {
      "font-sans": "Inter, sans-serif",
      "font-mono": "JetBrains Mono, monospace",
      "font-serif": "Source Serif 4, serif",
      "radius": "0.5rem",
      "tracking-tighter": "calc(var(--tracking-normal) - 0.05em)",
      "tracking-tight": "calc(var(--tracking-normal) - 0.025em)",
      "tracking-wide": "calc(var(--tracking-normal) + 0.025em)",
      "tracking-wider": "calc(var(--tracking-normal) + 0.05em)",
      "tracking-widest": "calc(var(--tracking-normal) + 0.1em)"
    },
    "light": {
      "background": "oklch(0.99 0 0)",      // rgb(252, 252, 252)
      "foreground": "oklch(0 0 0)",         // rgb(0, 0, 0)
      "card": "oklch(1 0 0)",               // rgb(255, 255, 255)
      "card-foreground": "oklch(0 0 0)",    // rgb(0, 0, 0)
      "popover": "oklch(1 0 0)",            // rgb(255, 255, 255)
      "popover-foreground": "oklch(0 0 0)", // rgb(0, 0, 0)
      "primary": "oklch(0.58 0.08 145)",    // rgb(109, 139, 111) - green
      "primary-foreground": "oklch(1 0 0)", // rgb(255, 255, 255)
      "secondary": "oklch(0.91 0 0)",       // rgb(232, 232, 232)
      "secondary-foreground": "oklch(0 0 0)", // rgb(0, 0, 0)
      "muted": "oklch(0.95 0 0)",           // rgb(243, 243, 243)
      "muted-foreground": "oklch(0.29 0 0)", // rgb(75, 75, 75)
      "accent": "oklch(0.58 0.08 145)",     // rgb(109, 139, 111) - green
      "accent-foreground": "oklch(1 0 0)",  // rgb(255, 255, 255)
      "destructive": "oklch(0.60 0.08 25)", // rgb(184, 131, 129)
      "destructive-foreground": "oklch(1 0 0)", // rgb(255, 255, 255)
      "border": "oklch(0.88 0 0)",          // rgb(223, 224, 223)
      "input": "oklch(0.91 0 0)",           // rgb(232, 232, 232)
      "ring": "oklch(0.58 0.08 145)",       // rgb(109, 139, 111) - green
      "chart-1": "oklch(0.58 0.08 145)",
      "chart-2": "oklch(0.50 0.18 200)",
      "chart-3": "oklch(0.45 0.16 250)",
      "chart-4": "oklch(0.40 0.14 300)",
      "chart-5": "oklch(0.35 0.12 350)",
      "radius": "0.5rem"
    },
    "dark": {
      "background": "oklch(0 0 0)",         // rgb(0, 0, 0)
      "foreground": "oklch(1 0 0)",         // rgb(255, 255, 255)
      "card": "oklch(0.05 0 0)",            // rgb(14, 14, 14)
      "card-foreground": "oklch(1 0 0)",    // rgb(255, 255, 255)
      "popover": "oklch(0.05 0 0)",         // rgb(14, 14, 14)
      "popover-foreground": "oklch(1 0 0)", // rgb(255, 255, 255)
      "primary": "oklch(0.68 0.06 145)",    // rgb(132, 166, 135) - green
      "primary-foreground": "oklch(1 0 0)", // rgb(255, 255, 255)
      "secondary": "oklch(0.13 0 0)",       // rgb(34, 34, 34)
      "secondary-foreground": "oklch(1 0 0)", // rgb(255, 255, 255)
      "muted": "oklch(0.11 0 0)",           // rgb(29, 29, 29)
      "muted-foreground": "oklch(0.61 0 0)", // rgb(155, 155, 155)
      "accent": "oklch(0.68 0.06 145)",     // rgb(132, 166, 135) - green
      "accent-foreground": "oklch(1 0 0)",  // rgb(255, 255, 255)
      "destructive": "oklch(0.70 0.06 25)", // rgb(203, 154, 151)
      "destructive-foreground": "oklch(0 0 0)", // rgb(0, 0, 0)
      "border": "oklch(0.14 0 0)",          // rgb(36, 36, 36)
      "input": "oklch(0.19 0 0)",           // rgb(48, 48, 48)
      "ring": "oklch(0.61 0 0)",            // rgb(155, 155, 155)
      "chart-1": "oklch(0.68 0.06 145)",
      "chart-2": "oklch(0.60 0.20 200)",
      "chart-3": "oklch(0.55 0.18 250)",
      "chart-4": "oklch(0.50 0.16 300)",
      "chart-5": "oklch(0.45 0.14 350)",
      "radius": "0.5rem"
    }
  }
};

function main() {
  const themeJsonPath = path.join(scriptDirectory, "..", "theme.json");
  
  console.log("Reading theme.json...");
  const themeData = JSON.parse(fs.readFileSync(themeJsonPath, "utf8")) as ThemeCatalog;
  
  console.log(`Found ${themeData.items.length} themes`);
  
  const dxIndex = themeData.items.findIndex(t => t.name === 'dx');
  
  if (dxIndex !== -1) {
    console.log("DX theme already exists, removing old version...");
    themeData.items.splice(dxIndex, 1);
  }
  
  themeData.items.unshift(dxTheme);
  
  console.log("Added DX theme at the top");
  
  fs.writeFileSync(themeJsonPath, JSON.stringify(themeData, null, 2), "utf8");
  
  console.log("Done. Updated theme.json");
  console.log(`  DX theme is now at position 1`);
  console.log(`  Total themes: ${themeData.items.length}`);
}

main();
