#!/usr/bin/env python3
"""Regenerate `crates/common/connect/src/static_nodes.rs`.

Static fallback inventories for the Connects surface, extracted from the real
upstream trees with the same rules as the runtime discovery code:

- n8n: class names from `packages/{nodes-base,@n8n/nodes-langchain} from
  `dist/known/nodes.json` (same `{package}.{className}` IDs as
  `discover_n8n_nodes`).
- flow-like: `(family, Name)` pairs from `impl NodeLogic for {Name}` markers
  in `packages/catalog/{family}/**/*.rs` (same
  `flow-like.{family}.{lower}` IDs as `collect_flow_logic`).

Usage:
    python crates/common/connect/scripts/regen_static_catalog.py \
        --n8n-root G:/HEXXED-8_20_2026/n8n \
        --flow-like-root <flow-like checkout> \
        [--flow-sha <commit, recorded in the header>]

Both roots are optional: pass only what changed. The script rewrites
`src/static_nodes.rs` in place; run `cargo test -p dx-connect` after.
"""

import argparse
import json
import os
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "src", "static_nodes.rs")
FLOW_FAMILIES = [
    "automation", "core", "data", "geo", "llm", "media",
    "ml", "onnx", "processing", "std", "web",
]


def n8n_classes(root):
    out = {}
    for package in ("nodes-base", "@n8n/nodes-langchain"):
        if package == "nodes-base":
            inv = os.path.join(root, "packages", "nodes-base", "dist", "known", "nodes.json")
        else:
            inv = os.path.join(root, "packages", "@n8n", "nodes-langchain", "dist", "known", "nodes.json")
        with open(inv, encoding="utf-8") as fh:
            data = json.load(fh)
        names = [v.get("className") or k for k, v in data.items()]
        out[package] = names
        print("%s: %d classes" % (package, len(names)), file=sys.stderr)
    return out


def flow_like_nodes(root):
    rows = []
    seen = set()
    catalog = os.path.join(root, "packages", "catalog")
    for family in FLOW_FAMILIES:
        fdir = os.path.join(catalog, family)
        if not os.path.isdir(fdir):
            print("warning: missing family dir %s" % fdir, file=sys.stderr)
            continue
        for dp, dn, fn in os.walk(fdir):
            if "tests" in dn:
                dn.remove("tests")
            for f in sorted(fn):
                if not f.endswith(".rs"):
                    continue
                try:
                    with open(os.path.join(dp, f), encoding="utf-8", errors="replace") as fh:
                        txt = fh.read()
                except OSError:
                    continue
                marker = "impl NodeLogic for "
                start = 0
                while True:
                    k = txt.find(marker, start)
                    if k < 0:
                        break
                    s = k + len(marker)
                    e = s
                    while e < len(txt) and (txt[e].isalnum() or txt[e] == "_"):
                        e += 1
                    name = txt[s:e]
                    start = e
                    if not name:
                        continue
                    nid = "flow-like.%s.%s" % (family, name.lower())
                    if nid in seen:
                        continue
                    seen.add(nid)
                    rows.append((family, name))
    print("flow-like: %d nodes" % len(rows), file=sys.stderr)
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n8n-root", default=os.environ.get("DX_N8N_ROOT", ""))
    ap.add_argument("--flow-like-root", default=os.environ.get("DX_FLOW_LIKE_ROOT", ""))
    ap.add_argument("--flow-sha", default="",
                    help="flow-like commit the extraction came from (recorded in header)")
    args = ap.parse_args()

    lines = []
    lines.append("//! Static fallback inventories (deterministic; used when no live")
    lines.append("//! checkout or materialized node folders are available).")
    lines.append("//!")
    lines.append("//! GENERATED — do not hand-edit. Regenerate with")
    lines.append("//! `crates/common/connect/scripts/regen_static_catalog.py` (see CATALOG.md).")
    lines.append("//! Sources:")
    lines.append("//! - n8n nodes-base + @n8n/nodes-langchain class names from")
    lines.append("//!   `packages/{nodes-base,@n8n/nodes-langchain}/dist/known/nodes.json`.")
    if args.flow_sha:
        lines.append("//! - flow-like `impl NodeLogic for X` nodes across the catalog")
        lines.append("//!   families, extracted with the same marker scan as")
        lines.append("//!   `collect_flow_logic` (flow-like @ %s)." % args.flow_sha)
    else:
        lines.append("//! - flow-like `impl NodeLogic for X` nodes across the catalog")
        lines.append("//!   families, extracted with the same marker scan as")
        lines.append("//!   `collect_flow_logic`.")
    lines.append("")

    if not args.n8n_root or not args.flow_like_root:
        ap.error("pass both --n8n-root and --flow-like-root (full regen only)")

    n8n = n8n_classes(args.n8n_root)
    lines.append("const N8N_BASE_NODES: &[&str] = &[")
    for n in n8n["nodes-base"]:
        lines.append('    "%s",' % n)
    lines.append("];")
    lines.append("")
    lines.append("const N8N_LANGCHAIN_NODES: &[&str] = &[")
    for n in n8n["@n8n/nodes-langchain"]:
        lines.append('    "%s",' % n)
    lines.append("];")
    lines.append("")
    rows = flow_like_nodes(args.flow_like_root)
    lines.append("const FLOW_LIKE_NODES: &[(&str, &str)] = &[")
    for fam, name in rows:
        lines.append('    ("%s", "%s"),' % (fam, name))
    lines.append("];")
    lines.append("")

    with open(OUT, "w", encoding="utf-8", newline="") as fh:
        fh.write("\n".join(lines) + "\n")
    print("wrote %s" % OUT)


if __name__ == "__main__":
    main()
