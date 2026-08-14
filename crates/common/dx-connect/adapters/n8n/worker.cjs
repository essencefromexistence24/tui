/*
 * DX Connect n8n adapter.
 *
 * This process is deliberately kept outside the TUI. It loads n8n's compiled
 * node classes and executes one node through n8n's WorkflowExecute engine for
 * each JSONL request. stdout is protocol-only; diagnostics go to stderr.
 */
'use strict';

const fs = require('node:fs');
const Module = require('node:module');
const path = require('node:path');
const readline = require('node:readline');

const PROTOCOL = 'dx-connect/1';
const root = path.resolve(process.env.DX_N8N_ROOT || process.cwd());

// A source checkout may have workspace junctions created for a different
// checkout path. Prefer the selected root and make its workspace packages
// resolvable without mutating the checkout or copying node_modules.
process.env.NODE_PATH = [
  path.join(root, 'node_modules'),
  path.join(root, 'packages'),
  path.join(root, 'packages', '@n8n'),
  path.join(root, 'packages', 'workflow', 'dist', 'cjs'),
  process.env.NODE_PATH || '',
].filter(Boolean).join(path.delimiter);
Module._initPaths();
const pnpmPackageRoots = new Map();
const pnpmRoot = path.join(root, 'node_modules', '.pnpm');
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
            pnpmPackageRoots.set(`${dependency.name}/${scoped.name}`, path.join(scope, scoped.name));
          }
        }
      } else {
        pnpmPackageRoots.set(dependency.name, path.join(dependencies, dependency.name));
      }
    }
  }
}

const originalResolveFilename = Module._resolveFilename;
function linkedWorkspacePackage(parent, packageName) {
  if (!parent?.filename) return undefined;
  let directory = path.dirname(parent.filename);
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
      // Continue searching parent workspace package directories.
    }
    const parentDirectory = path.dirname(directory);
    if (parentDirectory === directory) break;
    directory = parentDirectory;
  }
  return undefined;
}

function packageEntry(packageRoot) {
  try {
    const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
    if (typeof packageJson.main === 'string') return path.join(packageRoot, packageJson.main);
    const exported = packageJson.exports && (packageJson.exports['.'] || packageJson.exports);
    const nodeExport = exported?.node;
    if (typeof nodeExport === 'string') return path.join(packageRoot, nodeExport);
    if (typeof nodeExport?.require === 'string') return path.join(packageRoot, nodeExport.require);
    if (typeof exported?.require === 'string') return path.join(packageRoot, exported.require);
    if (typeof exported?.default === 'string') return path.join(packageRoot, exported.default);
  } catch {
    // Fall through to the conventional entry point.
  }
  return path.join(packageRoot, 'index.js');
}

Module._resolveFilename = function resolveFilename(request, parent, isMain, options) {
  if (request === 'n8n-workflow') {
    return path.join(root, 'packages', 'workflow', 'dist', 'cjs', 'index.js');
  }
  try {
    return originalResolveFilename.call(this, request, parent, isMain, options);
  } catch (error) {
    const parts = request.split('/');
    const packageName = request.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
    const packageRoot = linkedWorkspacePackage(parent, packageName) || pnpmPackageRoots.get(packageName);
    if (packageRoot) {
      const subpath = parts.slice(request.startsWith('@') ? 2 : 1).join('/');
      const candidate = subpath ? path.join(packageRoot, subpath) : packageEntry(packageRoot);
      return originalResolveFilename.call(this, candidate, parent, isMain, options);
    }
    throw error;
  }
};

// n8n packages and some third-party nodes are noisy during module loading.
// Keep stdout reserved for exactly one response frame per request.
console.log = (...args) => console.error(...args);
console.info = (...args) => console.error(...args);

function fail(requestId, message, runtimeVersion) {
  return {
    protocol: PROTOCOL,
    request_id: requestId,
    ok: false,
    error: String(message),
    runtime_version: runtimeVersion,
  };
}

function writeResponse(response) {
  process.stdout.write(`${JSON.stringify(response)}\n`);
}

function resolvePackageRoot(packageName) {
  const configured = process.env.DX_N8N_PACKAGES;
  const packageRoots = configured ? JSON.parse(configured) : {};
  if (packageRoots && typeof packageRoots[packageName] === 'string') {
    return path.resolve(packageRoots[packageName]);
  }
  if (packageName === 'n8n-nodes-base') {
    return path.join(root, 'packages', 'nodes-base');
  }
  if (packageName === '@n8n/n8n-nodes-langchain') {
    return path.join(root, 'packages', '@n8n', 'nodes-langchain');
  }
  throw new Error(`unsupported n8n node package: ${packageName}`);
}

let runtime;
let runtimePromise;

function debug(message) {
  if (process.env.DX_N8N_ADAPTER_DEBUG === '1') process.stderr.write(`[dx-n8n] ${message}\n`);
}

async function loadRuntime() {
  if (runtime) return runtime;
  if (runtimePromise) return await runtimePromise;

  runtimePromise = (async () => {
    debug(`loading runtime from ${root}`);
    const core = {
      WorkflowExecute: require(path.join(
        root,
        'packages',
        'core',
        'dist',
        'execution-engine',
        'workflow-execute.js',
      )).WorkflowExecute,
      LazyPackageDirectoryLoader: require(path.join(
        root,
        'packages',
        'core',
        'dist',
        'nodes-loader',
        'lazy-package-directory-loader.js',
      )).LazyPackageDirectoryLoader,
    };
    debug('loaded n8n-core');
    const workflow = require(path.join(root, 'packages', 'workflow', 'dist', 'cjs', 'index.js'));
    debug('loaded n8n-workflow');
    const loaders = new Map();

    for (const packageName of ['n8n-nodes-base', '@n8n/n8n-nodes-langchain']) {
      let packageRoot;
      try {
        packageRoot = resolvePackageRoot(packageName);
      } catch {
        continue;
      }
      if (!fs.existsSync(path.join(packageRoot, 'package.json'))) continue;
      debug(`loading node index for ${packageName}`);
      const loader = new core.LazyPackageDirectoryLoader(packageRoot);
      // Loading the 8 MiB `types/nodes.json` file is unnecessary for direct
      // execution. The loader can resolve and compile only the requested node
      // when its known index is populated.
      loader.known.nodes = JSON.parse(
        fs.readFileSync(path.join(packageRoot, 'dist', 'known', 'nodes.json'), 'utf8'),
      );
      loaders.set(packageName, loader);
    }

    if (loaders.size === 0) {
      throw new Error(`no n8n node packages found under ${root}`);
    }

  class NodeTypes {
    getLoader(type) {
      const separator = type.indexOf('.');
      if (separator <= 0) throw new Error(`invalid n8n node type: ${type}`);
      const packageName = type.slice(0, separator);
      const nodeName = type.slice(separator + 1);
      const loader = loaders.get(packageName);
      if (!loader) throw new Error(`n8n package is not loaded: ${packageName}`);
      return { loader, nodeName };
    }

    resolveType(type) {
      const separator = type.indexOf('.');
      if (separator <= 0) throw new Error(`invalid n8n node type: ${type}`);
      const packageName = type.slice(0, separator);
      const requestedName = type.slice(separator + 1);
      const loader = loaders.get(packageName);
      if (!loader) throw new Error(`n8n package is not loaded: ${packageName}`);
      const knownNames = Object.keys(loader.known.nodes);
      const resolvedName = knownNames.find((name) => name === requestedName)
        || knownNames.find((name) => name.toLowerCase() === requestedName.toLowerCase());
      if (!resolvedName) throw new Error(`unknown n8n node type: ${type}`);
      return `${packageName}.${resolvedName}`;
    }

    getByName(type) {
      const resolved = this.resolveType(type);
      const { loader, nodeName } = this.getLoader(resolved);
      return loader.getNode(nodeName).type;
    }

    getByNameAndVersion(type, version) {
      return workflow.NodeHelpers.getVersionedNodeType(this.getByName(type), version);
    }

    getKnownTypes() {
      const known = {};
      for (const [packageName, loader] of loaders) {
        for (const [name, value] of Object.entries(loader.known.nodes)) {
          known[`${packageName}.${name}`] = value;
        }
      }
      return known;
    }
  }

    runtime = {
      core,
      workflow,
      nodeTypes: new NodeTypes(),
      version: require(path.join(root, 'packages', 'core', 'package.json')).version,
    };
    debug('n8n runtime ready');
    return runtime;
  })();
  try {
    return await runtimePromise;
  } finally {
    if (!runtime) runtimePromise = undefined;
  }
}

function asNodeItems(items) {
  return (Array.isArray(items) ? items : []).map((item) => ({
    json: item && item.json && typeof item.json === 'object' ? item.json : {},
    ...(item && item.binary ? { binary: item.binary } : {}),
  }));
}

function makeHooks() {
  const handlers = {};
  return {
    handlers,
    addHandler(name, handler) {
      (handlers[name] ||= []).push(handler);
    },
    async runHook(name, args) {
      for (const handler of handlers[name] || []) await handler(...args);
    },
  };
}

class RuntimeCredentialsHelper {
  constructor(credentials, workflow) {
    this.credentials = credentials && typeof credentials === 'object' ? credentials : {};
    this.workflow = workflow;
  }

  getParentTypes() {
    return [];
  }

  isCredentialUsableByNode() {
    return true;
  }

  async authenticate(_credentials, _typeName, requestOptions) {
    return requestOptions;
  }

  async preAuthentication() {
    return undefined;
  }

  async runPreAuthentication() {
    return undefined;
  }

  async getDecrypted(_additionalData, _nodeCredentials, type) {
    return this.credentials[type] || {};
  }

  async getCredentials(nodeCredentials, type) {
    return new this.workflow.ICredentials(nodeCredentials, type, this.credentials[type] || {});
  }

  async updateCredentials() {}

  async updateCredentialsOauthTokenData() {}

  getCredentialsProperties() {
    return [];
  }
}

function makeAdditionalData(request, workflow) {
  const hooks = makeHooks();
  return {
    executionId: request.request_id,
    hooks,
    credentialsHelper: new RuntimeCredentialsHelper(request.context?.credentials, workflow),
    currentNodeParameters: undefined,
    parentCallbackManager: undefined,
    ssrfBridge: undefined,
    encryptedRunnerIdentity: undefined,
    evalLlmMockHandler: undefined,
    webhookWaitingBaseUrl: 'http://127.0.0.1:0/waiting-webhook',
    formWaitingBaseUrl: 'http://127.0.0.1:0/waiting-form',
    getRuntimeCredential: async (_runExecutionData, alias) => {
      return request.context?.credentials?.[alias] || undefined;
    },
  };
}

function makeNode(request, nodeType) {
  const parameters = { ...(request.context.parameters || {}) };
  const typeVersion = Number(parameters.__typeVersion || parameters.typeVersion || 1);
  delete parameters.__typeVersion;
  delete parameters.typeVersion;
  return {
    id: request.request_id,
    name: `DX Connect ${request.node_id}`,
    type: nodeType,
    typeVersion: Number.isFinite(typeVersion) ? typeVersion : 1,
    position: [0, 0],
    parameters,
    ...(request.context.credentials && Object.keys(request.context.credentials).length > 0
      ? { credentials: request.context.credentials }
      : {}),
  };
}

async function executeRequest(request) {
  if (!request || request.protocol !== PROTOCOL) {
    throw new Error(`unsupported protocol; expected ${PROTOCOL}`);
  }
  const loaded = await loadRuntime();
  const nodeType = loaded.nodeTypes.resolveType(request.node_id);
  const node = makeNode(request, nodeType);
  const workflowInstance = new loaded.workflow.Workflow({
    id: request.request_id,
    name: 'DX Connect isolated n8n node',
    nodes: [node],
    connections: {},
    active: false,
    nodeTypes: loaded.nodeTypes,
    settings: { executionOrder: 'v1' },
  });
  const inputItems = asNodeItems(request.context.items);
  const runExecutionData = loaded.workflow.createRunExecutionData({
    executionData: {
      waitingExecutionSource: null,
      nodeExecutionStack: [{
        node: workflowInstance.getStartNode(),
        data: { main: [inputItems] },
        source: null,
      }],
    },
  });
  const executor = new loaded.core.WorkflowExecute(
    makeAdditionalData(request, loaded.workflow),
    'manual',
    runExecutionData,
  );
  const result = await executor.processRunExecutionData(workflowInstance);
  const runs = result?.data?.resultData?.runData?.[node.name] || [];
  const lastRun = runs[runs.length - 1];
  const output = lastRun?.data?.main;
  if (!Array.isArray(output)) {
    throw new Error(lastRun?.error?.message || 'n8n node returned no main output');
  }
  return output.map((items) => asNodeItems(items));
}

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on('line', async (line) => {
  if (!line.trim()) return;
  let request;
  try {
    request = JSON.parse(line);
  } catch (error) {
    writeResponse(fail('unknown', `invalid JSON request: ${error}`));
    return;
  }
  try {
    const loaded = await loadRuntime();
    const outputs = await executeRequest(request);
    writeResponse({
      protocol: PROTOCOL,
      request_id: request.request_id,
      ok: true,
      outputs,
      runtime_version: loaded.version,
    });
  } catch (error) {
    writeResponse(fail(request.request_id || 'unknown', error?.stack || error));
  }
});
