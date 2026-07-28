import { execFileSync } from "node:child_process";

const disallowedScriptPatterns = [
  "scripts/*.js",
  "scripts/*.mjs",
  "scripts/*.cjs",
  "scripts/**/*.js",
  "scripts/**/*.mjs",
  "scripts/**/*.cjs",
];

const trackedFiles = execFileSync("git", ["ls-files", ...disallowedScriptPatterns], {
  encoding: "utf8",
})
  .split(/\r?\n/)
  .filter(Boolean);

if (trackedFiles.length > 0) {
  console.error("Authored TUI scripts must use TypeScript source files:");
  for (const file of trackedFiles) {
    console.error(`- ${file}`);
  }
  process.exitCode = 1;
}
