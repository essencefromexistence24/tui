import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(process.env.DX_N8N_ROOT || process.cwd());
const pnpmRoot = path.join(root, 'node_modules', '.pnpm');
const packageRoots = new Map();

if (fs.existsSync(pnpmRoot)) {
  for (const entry of fs.readdirSync(pnpmRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const dependencies = path.join(pnpmRoot, entry.name, 'node_modules');
    if (!fs.existsSync(dependencies)) continue;
    for (const dependency of fs.readdirSync(dependencies, { withFileTypes: true })) {
      if (!dependency.isDirectory()) continue;
      if (dependency.name.startsWith('@')) {
        const scope = path.join(dependencies, dependency.name);
        for (const scoped of fs.readdirSync(scope, { withFileTypes: true })) {
          if (scoped.isDirectory()) {
            packageRoots.set(`${dependency.name}/${scoped.name}`, path.join(scope, scoped.name));
          }
        }
      } else {
        packageRoots.set(dependency.name, path.join(dependencies, dependency.name));
      }
    }
  }
}

function linkedPackage(parentUrl, packageName) {
  if (!parentUrl?.startsWith('file:')) return undefined;
  let directory = path.dirname(fileURLToPath(parentUrl));
  while (directory.startsWith(root)) {
    const link = path.join(directory, 'node_modules', packageName);
    try {
      let target = fs.readlinkSync(link);
      if (!path.isAbsolute(target)) target = path.resolve(path.dirname(link), target);
      if (!fs.existsSync(target)) {
        const marker = `${path.sep}node_modules${path.sep}.pnpm${path.sep}`;
        const markerIndex = target.indexOf(marker);
        if (markerIndex >= 0) {
          target = path.join(root, 'node_modules', '.pnpm', target.slice(markerIndex + marker.length));
        }
      }
      if (fs.existsSync(target)) return target;
    } catch {
      // Keep walking workspace package parents.
    }
    const next = path.dirname(directory);
    if (next === directory) break;
    directory = next;
  }
  return undefined;
}

function packageEntry(packageRoot) {
  const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  const exported = packageJson.exports && (packageJson.exports['.'] || packageJson.exports);
  if (typeof exported?.import === 'string') return path.join(packageRoot, exported.import);
  if (typeof exported?.node?.import === 'string') return path.join(packageRoot, exported.node.import);
  if (typeof exported?.default === 'string') return path.join(packageRoot, exported.default);
  if (typeof packageJson.module === 'string') return path.join(packageRoot, packageJson.module);
  if (typeof packageJson.main === 'string') return path.join(packageRoot, packageJson.main);
  return path.join(packageRoot, 'index.js');
}

function exportTarget(value) {
  if (typeof value === 'string') return value;
  if (!value || typeof value !== 'object') return undefined;
  return value.import || value.node?.import || value.default || value.require || value.node?.require;
}

function resolvePackageSubpath(packageRoot, subpath) {
  const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  const target = exportTarget(packageJson.exports?.[`./${subpath}`]);
  if (target) return path.join(packageRoot, target);
  return path.join(packageRoot, subpath);
}

function resolveSubpath(candidate) {
  if (fs.existsSync(candidate) && !fs.statSync(candidate).isDirectory()) return candidate;
  for (const suffix of ['.js', '.mjs', '.cjs', path.sep + 'index.js']) {
    if (fs.existsSync(candidate + suffix)) return candidate + suffix;
  }
  return candidate;
}

export async function resolve(specifier, context, nextResolve) {
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    const parts = specifier.split('/');
    const packageName = specifier.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
    const packageRoot = linkedPackage(context.parentURL, packageName) || packageRoots.get(packageName);
    if (!packageRoot) throw error;
    const subpath = parts.slice(specifier.startsWith('@') ? 2 : 1).join('/');
    const candidate = resolveSubpath(
      subpath ? resolvePackageSubpath(packageRoot, subpath) : packageEntry(packageRoot),
    );
    return await nextResolve(pathToFileURL(candidate).href, context);
  }
}
