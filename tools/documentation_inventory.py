#!/usr/bin/env python3
"""Emit a compact documentation inventory (central.documentation-inventory/v1).

One bounded record per document — path, revision, role, standing, scope, a short
determination, relations, capability refs and addressable unit ids — so an Agent
(or a Jev attention question) can choose what to read before reading any body.
No document body is emitted. Roles come from explicit declarations first
(front matter `role:`, HTML provenance JSON or `data-doc-role`, `%%` headers),
then from filename conventions; anything else is `other`.

    python3 tools/documentation_inventory.py docs/ ProjectCentral/user
    python3 tools/documentation_inventory.py docs/ --upward cap.example.id
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import html
import io
import json
import re
import sys
from pathlib import Path, PurePosixPath

SCHEMA = "central.documentation-inventory/v1"
ROLES = ("vision", "design", "mockup", "architecture", "diagram", "capability-matrix", "other")
STANDINGS = ("authored-human-position", "design-commitment", "architecture-contract",
             "implementation-fact", "observed-evidence", "agent-inference")
DEFAULT_STANDING = "agent-inference"
REF_KEYS = ("vision_refs", "design_refs", "mockup_refs", "architecture_refs", "diagram_refs", "praxis_refs")
SUFFIXES = {".html", ".htm", ".md", ".mmd", ".csv"}
MAX_DETERMINATION = 240
MAX_ITEMS = 200
MATRIX_HEAD = ["id", "record_type", "view_id", "row_id", "column_id", "capability_refs"]
MERMAID_WORDS = {
    "flowchart", "graph", "subgraph", "end", "direction", "participant", "actor", "as", "sequencediagram",
    "statediagram", "statediagram-v2", "lr", "rl", "tb", "bt", "td", "note", "left", "right", "of", "over",
    "loop", "alt", "else", "opt", "par", "and", "rect", "state", "classdef", "class", "style", "linkstyle",
    "click", "autonumber", "activate", "deactivate",
}


def _bounded(items):
    unique = list(dict.fromkeys(items))
    return unique[:MAX_ITEMS], len(unique) > MAX_ITEMS


def _sentence(text: str) -> str:
    text = re.sub(r"\s+", " ", text).strip()
    match = re.match(r"(.+?[.!?])(\s|$)", text)
    return match.group(1) if match else text


def _determination(heading: str, body: str) -> str:
    parts = [p for p in (heading.strip(), _sentence(body)) if p]
    text = " — ".join(parts)
    return text if len(text) <= MAX_DETERMINATION else text[:MAX_DETERMINATION - 1].rstrip() + "…"


def _slug(heading: str) -> str:
    slug = re.sub(r"[^\w\- ]", "", heading.strip().lower())
    return re.sub(r"\s", "-", slug)


def _standing(value):
    return value if value in STANDINGS else DEFAULT_STANDING


def _front_matter(text: str):
    if not text.startswith("---\n"):
        return {}, text
    end = text.find("\n---", 4)
    if end < 0:
        return {}, text
    data = {}
    for line in text[4:end].splitlines():
        if ":" not in line or line.startswith((" ", "#")):
            continue
        key, value = line.split(":", 1)
        value = value.strip()
        if value.startswith("["):
            try:
                value = json.loads(value)
            except ValueError:
                value = [v.strip().strip("'\"") for v in value.strip("[]").split(",") if v.strip()]
        else:
            value = value.strip("'\"")
        data[key.strip()] = value
    return data, text[end + 4:].lstrip("\n")


def _role_from_name(path: Path):
    stem = path.stem.lower()
    for role in ("vision", "design", "mockup", "architecture"):
        if role in stem:
            return role
    return None


def _ref_relations(data):
    relations = []
    for key in REF_KEYS:
        values = data.get(key) or []
        if isinstance(values, str):
            values = [values]
        relations += [{"relation": key, "target": v} for v in values if isinstance(v, str) and v.strip()]
    for item in data.get("relations") or []:
        if isinstance(item, dict) and isinstance(item.get("relation"), str) and isinstance(item.get("target"), str):
            relations.append({"relation": item["relation"], "target": item["target"]})
    return relations


def _list(value):
    if isinstance(value, str):
        return [value] if value.strip() else []
    return [v for v in value or [] if isinstance(v, str) and v.strip() and not v.startswith("[")]


def read_markdown(path: Path, text: str):
    meta, body = _front_matter(text)
    headings = re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", body, re.MULTILINE)
    first = next(iter(headings), "")
    after = body.split(first, 1)[1] if first else body
    para = next((p for p in re.split(r"\n\s*\n", after) if p.strip() and not p.lstrip().startswith(("#", "|", "```", "-"))), "")
    return {
        "role": meta.get("role") if meta.get("role") in ROLES else _role_from_name(path),
        "standing": meta.get("standing"), "scope": meta.get("scope"),
        "determination": _determination(first, para),
        "relations": _ref_relations(meta),
        "capability_refs": _list(meta.get("capability_refs")),
        "unit_ids": [_slug(h) for h in headings],
    }


def _strip_tags(fragment: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", " ", fragment))


def read_html(path: Path, text: str):
    provenance = {}
    for match in re.finditer(r'<script type="application/json" id="[\w-]*provenance"[^>]*>(.*?)</script>', text, re.DOTALL):
        try:
            provenance = json.loads(match.group(1))
        except ValueError:
            pass
        break
    declared = re.search(r'<body[^>]*\sdata-doc-role="([\w-]+)"', text)
    role = provenance.get("role") or (declared.group(1) if declared else None)
    role = role if role in ROLES else _role_from_name(path)
    title = re.search(r"<h1[^>]*>(.*?)</h1>", text, re.DOTALL) or re.search(r"<title>(.*?)</title>", text, re.DOTALL)
    deck = re.search(r'<p class="hero-deck"[^>]*>(.*?)</p>', text, re.DOTALL) or re.search(r"<p[^>]*>(.*?)</p>", text, re.DOTALL)
    relations = _ref_relations(provenance)
    for attribute, key in (("data-design-ref", "design_refs"), ("data-vision-ref", "vision_refs"),
                           ("data-seed-ref", "vision_refs")):
        relations += [{"relation": key, "target": v} for v in re.findall(attribute + r'="([^"]+)"', text)
                      if not v.startswith("[")]
    capabilities = _list(provenance.get("capability_refs")) + [
        v for v in re.findall(r'data-capability-ref="([^"]+)"', text) if not v.startswith("[")]
    return {
        "role": role, "standing": provenance.get("standing"), "scope": provenance.get("scope"),
        "determination": _determination(_strip_tags(title.group(1)) if title else "",
                                         _strip_tags(deck.group(1)) if deck else ""),
        "relations": [dict(t) for t in dict.fromkeys(tuple(r.items()) for r in relations)],
        "capability_refs": capabilities,
        "unit_ids": re.findall(r'<(?:section|article)\b[^>]*\sid="([^"]+)"', text),
    }


def read_mermaid(path: Path, text: str):
    header = {}
    nodes = []
    kind = ""
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("%%"):
            if ":" in stripped:
                key, value = stripped[2:].split(":", 1)
                header[key.strip()] = value.strip()
            continue
        if not stripped:
            continue
        if not kind:
            kind = stripped.split()[0].lower()
        if kind in ("sequencediagram", "statediagram", "statediagram-v2"):
            stripped = stripped.split(":", 1)[0]
        stripped = re.sub(r'"[^"]*"|\|[^|]*\||\[[^\]]*\]|\([^)]*\)|\{[^}]*\}', " ", stripped)
        if stripped.split() and stripped.split()[0].lower() == "participant":
            stripped = stripped.split()[1] if len(stripped.split()) > 1 else ""
        nodes += [t for t in re.findall(r"[A-Za-z_][\w]*", stripped) if t.lower() not in MERMAID_WORDS]
    standing = next((s for s in STANDINGS if s in header.get("provenance", "")), None)
    companion = header.get("companion_doc", "")
    return {
        "role": "diagram", "standing": standing, "scope": header.get("source_id"),
        "determination": _determination(header.get("source_id", path.stem), header.get("visual_question", "")),
        "relations": [{"relation": "visualises", "target": companion}] if companion and not companion.startswith("[") else [],
        "capability_refs": [],
        "unit_ids": nodes,
    }


def read_csv(path: Path, text: str):
    rows = list(csv.DictReader(io.StringIO(text)))
    reader_header = next(csv.reader(io.StringIO(text)), [])
    if reader_header[:len(MATRIX_HEAD)] != MATRIX_HEAD:
        return {"role": _role_from_name(path), "standing": None, "scope": None,
                "determination": _determination(path.name, ""), "relations": [], "capability_refs": [], "unit_ids": []}
    capabilities = [r["id"] for r in rows if r.get("record_type") == "capability"]
    relations = []
    for row in rows:
        try:
            documentation = json.loads(row.get("extensions") or "{}").get("documentation", {})
        except (ValueError, AttributeError):
            continue
        if isinstance(documentation, dict):
            relations += [dict(r, capability=row["id"]) for r in _ref_relations(documentation)]
    return {
        "role": "capability-matrix", "standing": None, "scope": None,
        "determination": _determination(path.name, f"Capability matrix with {len(capabilities)} capabilities and {len(rows) - len(capabilities)} relations."),
        "relations": relations, "capability_refs": capabilities, "unit_ids": [r["id"] for r in rows],
    }


READERS = {".html": read_html, ".htm": read_html, ".md": read_markdown, ".mmd": read_mermaid, ".csv": read_csv}


def record(path: Path, root: Path):
    data = path.read_bytes()
    text = data.decode("utf-8", errors="replace")
    reading = READERS[path.suffix.lower()](path, text)
    unit_ids, units_truncated = _bounded(reading["unit_ids"])
    relations = reading["relations"][:MAX_ITEMS]
    capability_refs, _ = _bounded(reading["capability_refs"])
    out = {
        "source_ref": path.resolve().relative_to(root).as_posix(),
        "revision": "sha256:" + hashlib.sha256(data).hexdigest(),
        "role": reading["role"] or "other",
        "standing": _standing(reading["standing"]),
        "scope": reading["scope"] if isinstance(reading["scope"], str) and not reading["scope"].startswith("[") else None,
        "determination": reading["determination"],
        "relations": relations,
        "capability_refs": capability_refs,
        "unit_ids": unit_ids,
    }
    if units_truncated or len(reading["relations"]) > MAX_ITEMS:
        out["truncated"] = True
    return out


def inventory(paths, root: Path):
    root = root.resolve()
    files = []
    for given in paths:
        given = Path(given)
        candidates = [given] if given.is_file() else sorted(given.rglob("*"))
        for path in candidates:
            if (path.is_file() and path.suffix.lower() in SUFFIXES
                    and not any(part.startswith(".") for part in path.relative_to(given if given.is_dir() else given.parent).parts)):
                files.append(path)
    records = [record(path, root) for path in dict.fromkeys(p.resolve() for p in files)]
    return {"schema": SCHEMA, "records": records}


def resolve(records, source_ref: str, ref: str):
    """Resolve a `file#unit` reference made inside `source_ref` to an inventory record."""
    target = ref.split("#", 1)[0]
    if not target:
        return next((r for r in records if r["source_ref"] == source_ref), None)
    base = PurePosixPath(source_ref).parent
    parts = []
    for part in (base / target).parts:
        if part == "..":
            if parts:
                parts.pop()
        elif part != ".":
            parts.append(part)
    wanted = "/".join(parts)
    return next((r for r in records if r["source_ref"] == wanted), None)


UPWARD = {"capability-matrix": ("architecture_refs", "design_refs", "vision_refs"),
          "mockup": ("design_refs", "vision_refs"), "diagram": ("visualises",),
          "architecture": ("design_refs",), "design": ("vision_refs",), "vision": ()}


def upward(records, capability_id: str):
    """Reverse traversal from a capability record toward Vision via declared refs.

    Returns the ordered list of reached source_refs (architecture → design → vision
    when all exist). A missing layer is simply absent from the path.
    """
    matrix = next((r for r in records if r["role"] == "capability-matrix" and capability_id in r["capability_refs"]), None)
    if matrix is None:
        return []
    path, current = [], matrix
    refs = [r for r in matrix["relations"] if r.get("capability") == capability_id]
    while True:
        step = None
        for key in UPWARD.get(current["role"], ()):
            for relation in (refs if current is matrix else current["relations"]):
                if relation["relation"] == key:
                    step = resolve(records, current["source_ref"], relation["target"])
                    if step and step["source_ref"] not in path:
                        break
                    step = None
            if step:
                break
        if not step:
            return path
        path.append(step["source_ref"])
        current = step


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="source_ref base (default: current directory)")
    parser.add_argument("--upward", metavar="CAPABILITY_ID", help="print the reverse traversal from a capability")
    args = parser.parse_args()
    try:
        result = inventory(args.paths, args.root)
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    if args.upward:
        result = {"schema": SCHEMA, "capability": args.upward, "upward": upward(result["records"], args.upward)}
    json.dump(result, sys.stdout, indent=2, ensure_ascii=False)
    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
