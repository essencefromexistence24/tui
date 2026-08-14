# Dx logo assets

Generated on 2026-08-15 from the local Dx registries.

- Connect nodes inspected: 2,208 `node.json` entries.
- Channel entries inspected: 37 entries from the ZeroClaw channel registry.
- Individual node-name logos obtained: 228.
- Source-logo fallbacks added: 1,980 (1,644 Flow-Like nodes use the Flow-Like
  logo; 564 n8n nodes use the n8n logo).
- Connect assets now cover all 2,208 nodes; channel logos add 31 real PNGs.
- ASCII art covers all 2,208 connects and all 37 channel entries. The six
  channels without a Logo.dev match use deterministic labeled fallback art.

Files are named from the local connect folder or channel identifier:

- `connects/<node-folder>.png`
- `channels/<channel-name>.png`
- `ascii/connects/<node-folder>.txt`
- `ascii/channels/<channel-name>.txt`

ASCII assets are generated with the same `ascii-image-converter` pipeline used
by the Dx CLI:

```powershell
pwsh -NoProfile -File scripts/generate-logo-ascii.ps1 `
  -SourceRoot logos/connects -OutputRoot logos/ascii/connects
pwsh -NoProfile -File scripts/generate-logo-ascii.ps1 `
  -SourceRoot logos/channels -OutputRoot logos/ascii/channels
```

The assets were fetched from Logo.dev's name-based image endpoint at 128px PNG
size with `fallback=404`. The publishable key was used only during the download
and is not stored here. The Logo.dev secret key is not needed for the image CDN;
it is reserved for server-side search APIs and was not written to this project.

When a node name was not itself a recognized brand, its owning source registry
logo was copied to that node's filename. This keeps every node renderable while
avoiding a misleading monogram or an unrelated brand match.
