#!/usr/bin/env python3
"""Check the Central authored-ground pilot, without network access or dependencies.

This checks structural integrity and source discoverability, not human ratification
or the truth of a claim. Resource paths are relative to the selected HTML account.
"""
from __future__ import annotations

import argparse
import csv
from html.parser import HTMLParser
import io
import hashlib
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capability_matrix
from urllib.parse import unquote, urlsplit

VESSELS = {
    "readme", "founding-position", "vision", "constitution-design-commitment",
    "design-commitment", "architecture-contract", "protocol-method",
    "surface-discovery", "implementation-guide", "evidence", "changelog",
    "temporal-working-state", "research-notes-transcripts", "research-transcript",
    "skill-agent-operating-instruction", "skill-instruction",
}
STANDINGS = {
    "authored-human-position", "design-commitment", "architecture-contract",
    "implementation-fact", "observed-evidence", "agent-inference",
}
COLUMNS = capability_matrix.COLUMNS
CAPABILITY_COLUMNS = ["need", "operation", "outcome", "implementation_status", "standing", "source_refs", "code_refs", "test_refs", "account_ref"]
WIKILINK = re.compile(r"\[\[([^\[\]\n]+)\]\]")
PLACEHOLDER = re.compile(
    r"\[(?:Content(?:\.[^\]]*)?|[^\[\]]*SURFACE TITLE|ACCOUNT TITLE|"
    r"SHORT ACCOUNT NAME|ONE-SENTENCE ACCOUNT DESCRIPTION|PURPOSE / AUDIENCE / STATUS|account-uid|"
    r"PROJECT OR CONTEXT|SESSION ID OR UNKNOWN|YYYY-MM-DDTHH:MM:SS\+TZ|"
    r"MODEL / AGENT / SKILL VERSION|Natural [^\]]*|Optional [^\]]*|"
    r"Section title|Section-relative context appears here\.)\]"
    r"|\b(?:TODO|FIXME|TBD)\b|https?://example\.com\b"
)


class Account(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids: list[str] = []
        self.hrefs: list[str] = []
        self.surfaces: list[str] = []
        self.surface_attrs: dict[str, dict] = {}
        self.seed_text: dict[str, str] = {}
        self.seed_key: str | None = None
        self.seed_depth = 0
        self.nodes: list[tuple[str | None, dict]] = []
        self.scripts: dict[str, str] = {}
        self.prose: list[str] = []
        self.surface: str | None = None
        self.script: str | None = None
        self.in_style = False

    def handle_starttag(self, tag, attrs):
        attr = dict(attrs)
        if tag == "a" and "href" in attr:
            self.hrefs.append(attr["href"])
        if attr.get("id"):
            self.ids.append(attr["id"])
        if tag == "article" and "surface" in attr.get("class", "").split():
            self.surface = attr.get("data-surface")
            self.surfaces.append(self.surface)
            self.surface_attrs[self.surface] = attr
        if tag == "section" and "ql-node" in attr.get("class", "").split():
            self.nodes.append((self.surface, attr))
        if tag == "div":
            if self.seed_key is not None:
                self.seed_depth += 1
            elif attr.get("data-seed-content"):
                self.seed_key = attr["data-seed-content"]
                self.seed_depth = 1
                self.seed_text[self.seed_key] = ""
        if tag == "script":
            self.script = attr.get("id", "__anonymous__")
            self.scripts.setdefault(self.script, "")
        if tag == "style":
            self.in_style = True

    def handle_endtag(self, tag):
        if self.seed_key is not None:
            if tag == "p":
                self.seed_text[self.seed_key] += " "
            if tag == "div":
                self.seed_depth -= 1
                if self.seed_depth == 0:
                    self.seed_key = None
        if tag == "article":
            self.surface = None
        if tag == "script":
            self.script = None
        if tag == "style":
            self.in_style = False

    def handle_data(self, data):
        if self.seed_key is not None:
            self.seed_text[self.seed_key] += data
        if self.script is not None:
            self.scripts[self.script] += data
        elif not self.in_style:
            self.prose.append(data)


def read_account(path: Path) -> Account:
    account = Account()
    account.feed(path.read_text(encoding="utf-8"))
    return account


def duplicates(items):
    seen = set()
    repeated = set()
    for item in items:
        if item in seen:
            repeated.add(item)
        seen.add(item)
    return repeated


def fragment_exists(path: Path, fragment: str, account: Account | None = None):
    if path.suffix.lower() in {".html", ".htm"}:
        parsed = account or read_account(path)
        routes = {f"{surface}/{attrs.get('data-route')}" for surface, attrs in parsed.nodes}
        return fragment in parsed.ids or fragment in routes or fragment in parsed.surfaces
    text = path.read_text(encoding="utf-8")
    if path.suffix.lower() in {".md", ".markdown"}:
        headings = re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.MULTILINE)
        anchors = set()
        counts = {}
        for heading in headings:
            slug = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            anchors.add(f"{slug}-{count}" if count else slug)
        return fragment in anchors or bool(re.search(r'(?:id|name)=[\"\']' + re.escape(fragment) + r'[\"\']', text))
    return False


def validate(root: Path, account_name: str = "central.html", product_index: int = 0, namespace: str = "central", reference_scope: str = "workspace") -> list[str]:
    errors = []
    root = root.resolve()
    if reference_scope not in {"workspace", "repository"}:
        return ["Unknown reference scope"]
    def local_check(path):
        return reference_scope == "workspace" or path.resolve().is_relative_to(root)
    html = root / "ProjectCentral/user" / account_name
    try:
        raw = html.read_text(encoding="utf-8")
        account = read_account(html)
    except (OSError, UnicodeError) as exc:
        return [f"Cannot read account: {exc}"]
    for value in sorted(duplicates(account.ids)):
        errors.append(f"Duplicate HTML id: {value}")
    if sorted(account.surfaces, key=str) != [f"q{i}" for i in range(6)] + ["whole"]:
        errors.append("Account requires the 0/1 whole overview and exactly six expanded surfaces q0 through q5")
    if account.surface_attrs.get("whole", {}).get("data-whole-anchor") != "0/1":
        errors.append("Overview must declare the 0/1 whole anchor")
    expected_seeds = {f"{namespace}:seed:q{i}" for i in range(6)}
    if set(account.seed_text) != expected_seeds:
        errors.append("Overview requires six addressable seed answers")
    for i in range(6):
        key = f"{namespace}:seed:q{i}"
        attrs = account.surface_attrs.get(f"q{i}", {})
        if attrs.get("data-seed-ref") != key:
            errors.append(f"Expanded q{i} must reference its corresponding seed {key}")
        content = " ".join(account.seed_text.get(key, "").split())
        digest = hashlib.sha256(content.encode()).hexdigest()
        if not content or attrs.get("data-seed-sha256") != digest:
            errors.append(f"Seed changed: reconcile expanded q{i} with the current overview answer")
    routes = []
    for surface, attrs in account.nodes:
        label = attrs.get("id", "<missing id>")
        if not attrs.get("id") or not attrs.get("data-route"):
            errors.append(f"Node {label} requires id and data-route")
        routes.append(f"{surface}/{attrs.get('data-route')}")
        if surface not in {f"q{i}" for i in range(6)} | {"whole"}:
            errors.append(f"Node {label} is outside a QL surface")
        if attrs.get("data-node") not in {f".{i}" for i in range(6)}:
            errors.append(f"Node {label} has invalid data-node")
        if attrs.get("data-vessel") not in VESSELS:
            errors.append(f"Node {label} has unknown vessel: {attrs.get('data-vessel')}")
        if attrs.get("data-standing") not in STANDINGS:
            errors.append(f"Node {label} has unknown standing: {attrs.get('data-standing')}")
        if not attrs.get("data-tags", "").strip():
            errors.append(f"Node {label} requires nonempty data-tags")
    for surface in account.surfaces:
        if not any(s == surface for s, _ in account.nodes):
            errors.append(f"Surface {surface} has no authored nodes")
    for route in sorted(duplicates(routes)):
        errors.append(f"Duplicate QL route: {route}")
    data = {}
    for script in ("account-provenance", "ground-resources"):
        try:
            data[script] = json.loads(account.scripts[script])
            if not isinstance(data[script], dict):
                raise ValueError("expected a JSON object")
        except (KeyError, ValueError) as exc:
            errors.append(f"Invalid {script}: {exc}")
            data[script] = {}
    provenance = data["account-provenance"]
    if not isinstance(provenance.get("source_basis"), list) or not provenance.get("source_basis"):
        errors.append("account-provenance requires a nonempty source_basis list")
    resources = data["ground-resources"]
    for key, entry in resources.items():
        if not isinstance(entry, dict) or not entry.get("href") or not entry.get("title"):
            errors.append(f"Resource {key} requires href and title")
            continue
        if not isinstance(entry["href"], str):
            errors.append(f"Resource {key} href must be text")
            continue
        parts = urlsplit(entry["href"])
        if parts.scheme or parts.netloc:
            errors.append(f"Resource {key} must point to a local source: {entry['href']}")
            continue
        path = (html.parent / unquote(parts.path)).resolve() if parts.path else html
        if not local_check(path):
            continue
        if not path.is_file():
            errors.append(f"Unresolved resource {key}: {entry['href']}")
        elif parts.fragment:
            try:
                if not fragment_exists(path, unquote(parts.fragment), account if path == html else None):
                    errors.append(f"Unresolved fragment for resource {key}: {entry['href']}")
            except (OSError, UnicodeError) as exc:
                errors.append(f"Cannot inspect resource {key}: {exc}")
    for href in account.hrefs:
        parts = urlsplit(href)
        if parts.scheme or parts.netloc:
            continue
        path = (html.parent / unquote(parts.path)).resolve() if parts.path else html
        if not local_check(path):
            continue
        if not path.is_file():
            errors.append(f"Unresolved HTML link: {href}")
        elif parts.fragment:
            try:
                if not fragment_exists(path, unquote(parts.fragment), account if path == html else None):
                    errors.append(f"Unresolved HTML link fragment: {href}")
            except (OSError, UnicodeError) as exc:
                errors.append(f"Cannot inspect HTML link {href}: {exc}")
    for i in range(6):
        key = f"{namespace}:seed:q{i}"
        nodes = [attrs for surface, attrs in account.nodes if surface == "whole" and attrs.get("data-resource") == key]
        if len(nodes) != 1 or resources.get(key, {}).get("href") != "#whole/" + nodes[0].get("data-route", ""):
            errors.append(f"Seed resource {key} must resolve to its overview answer")
    for link in WIKILINK.findall("".join(account.prose)):
        key = link.split("|", 1)[0].strip()
        if key not in resources:
            errors.append(f"Unknown wikilink resource: {key}")
    match = PLACEHOLDER.search(raw)
    if match:
        errors.append(f"Unreplaced template or work placeholder: {match.group()}")
    csv_path = html.parent / "capability-matrix.csv"
    md_path = html.parent / "capability-matrix.md"
    try:
        csv_text = csv_path.read_text(encoding="utf-8")
        md = md_path.read_text(encoding="utf-8")
        rows = list(csv.reader(io.StringIO(csv_text), strict=True))
        manifest_path = html.parent / "capability-matrix.json"
        errors.extend(capability_matrix.validate(manifest_path, csv_path))
        try:
            manifest, header, records = capability_matrix.load(manifest_path, csv_path)
        except (OSError, UnicodeError, ValueError, csv.Error):
            manifest, header, records = {}, [], []
        if manifest and not capability_matrix.validate_data(manifest, header, records):
            errors.extend(capability_matrix.validate_product_profile(manifest, product_index, namespace))
        if header[:len(COLUMNS)] == COLUMNS:
            capability_count = 0
            for record in records:
                key = record["id"]
                kind = record["record_type"]
                if kind == "relation" and record["view_id"] == "product-field":
                    for ref in filter(None, (value.strip() for value in record["source_refs"].split(";"))):
                        parts = urlsplit(ref)
                        if parts.scheme or parts.netloc or Path(unquote(parts.path)).is_absolute():
                            errors.append(f"Product relation {key} requires repository-relative source_refs: {ref}")
                            continue
                        target = (root / unquote(parts.path)).resolve()
                        if not local_check(target):
                            continue
                        if not target.is_file():
                            errors.append(f"Product relation {key} has unresolved source_refs: {ref}")
                        elif parts.fragment and not fragment_exists(target, unquote(parts.fragment)):
                            errors.append(f"Product relation {key} has unresolved source_refs fragment: {ref}")
                    parts = urlsplit(record["account_ref"])
                    target = (html.parent / unquote(parts.path)).resolve()
                    if (parts.scheme or parts.netloc or Path(unquote(parts.path)).is_absolute()
                            or target != html.resolve() or not parts.fragment
                            or not fragment_exists(html, unquote(parts.fragment), account)):
                        errors.append(f"Product relation {key} requires a resolvable account_ref into {account_name}")
                    try:
                        extensions = json.loads(record["extensions"])
                    except ValueError:
                        extensions = {}
                    if isinstance(extensions, dict):
                        if "seed_ref" in extensions and extensions["seed_ref"] != f"{namespace}:seed:{record['row_id']}":
                            errors.append(f"Product relation {key} seed_ref must match its manifest row")
                        if "source_unit" in extensions and extensions["source_unit"] not in account.ids:
                            errors.append(f"Product relation {key} source_unit must identify an actual HTML unit")
                if kind == "capability":
                    capability_count += 1
                    if not re.fullmatch(r"cap\." + re.escape(namespace) + r"\.[a-z0-9]+(?:-[a-z0-9]+)*", key):
                        errors.append(f"Capability {key} requires a cap.{namespace}.<slug> identity")
                    intent_only = record["implementation_status"].split(";", 1)[0].strip() in {"intent-only", "unimplemented", "unresolved"}
                    for field in CAPABILITY_COLUMNS:
                        if intent_only and field in {"code_refs", "test_refs"}:
                            continue
                        if not record[field].strip():
                            errors.append(f"Capability {key} requires nonempty {field}")
                    for field in ("source_refs", "code_refs", "test_refs"):
                        if intent_only and field in {"code_refs", "test_refs"} and not record[field].strip():
                            continue
                        for ref in record[field].split(";"):
                            ref = ref.strip()
                            parts = urlsplit(ref)
                            if not ref or parts.scheme or parts.netloc or Path(unquote(parts.path)).is_absolute():
                                errors.append(f"Capability {key} {field} requires repository-relative file references: {ref}")
                                continue
                            target = (root / unquote(parts.path)).resolve()
                            if not local_check(target):
                                continue
                            if not target.is_file():
                                errors.append(f"Capability {key} has unresolved {field}: {ref}")
                            elif parts.fragment and not fragment_exists(target, unquote(parts.fragment)):
                                errors.append(f"Capability {key} has unresolved {field} fragment: {ref}")
                    parts = urlsplit(record["account_ref"])
                    target = (html.parent / unquote(parts.path)).resolve()
                    if (parts.scheme or parts.netloc or Path(unquote(parts.path)).is_absolute()
                            or target != html.resolve() or not parts.fragment
                            or not fragment_exists(html, unquote(parts.fragment), account)):
                        errors.append(f"Capability {key} requires a resolvable account_ref into {account_name}")
                    anchor = key.replace(".", "-")
                    if not fragment_exists(md_path, anchor):
                        errors.append(f"Capability {key} requires matrix MD anchor {anchor}")
                    entry = resources.get(key)
                    expected = f"capability-matrix.md#{anchor}"
                    if not isinstance(entry, dict) or entry.get("href") != expected:
                        errors.append(f"Capability {key} requires HTML resource linkage to {expected}")
            if not capability_count:
                errors.append("Matrix requires at least one functional capability row")
            for key in sorted(duplicates(row[0] for row in rows[1:] if row)):
                errors.append(f"Duplicate matrix id: {key}")
        blocks = re.findall(r"^```csv\s*\n(.*?)^```\s*$", md, re.MULTILINE | re.DOTALL)
        if len(blocks) != 1:
            errors.append("Matrix MD requires exactly one lossless fenced csv appendix")
        elif blocks[0].rstrip("\n") != csv_text.rstrip("\n"):
            errors.append("Matrix MD/CSV drift: fenced appendix differs from canonical CSV")
        elif list(csv.reader(io.StringIO(blocks[0]), strict=True)) != rows:
            errors.append("Matrix MD/CSV parsed rows differ")
        if not re.search(r"^\|.*\|\s*\n\|[ :|\-]+\|", md, re.MULTILINE):
            errors.append("Matrix MD requires a human-readable table")
    except (OSError, UnicodeError, csv.Error) as exc:
        errors.append(f"Cannot read matrix pair: {exc}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--account", default="central.html")
    parser.add_argument("--product-index", type=int, choices=range(6), default=0)
    parser.add_argument("--namespace", default="central")
    args = parser.parse_args()
    errors = validate(args.root.resolve(), args.account, args.product_index, args.namespace)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print(f"{args.namespace} product ground: structure, capability traces, declared matrix views and MD/CSV parity verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
