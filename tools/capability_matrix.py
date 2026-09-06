#!/usr/bin/env python3
"""Validate ql-capability-matrix/1 carriers independently of any product profile.

Unknown appended CSV fields and manifest properties are retained by load(). A
view gives addresses meaning; records do not acquire meaning from grid size.
"""
from __future__ import annotations
import argparse
import csv
import io
import json
from pathlib import Path
import re

PROTOCOL = "ql-capability-matrix/1"
COLUMNS = "id record_type view_id row_id column_id capability_refs need operation outcome implementation_status standing source_refs code_refs test_refs account_ref relation coverage extensions".split()
STANDINGS = {"authored-human-position", "design-commitment", "architecture-contract", "implementation-fact", "observed-evidence", "agent-inference"}


def load(manifest_path: Path, csv_path: Path | None = None):
    """Return manifest, original header, and records without dropping extra fields."""
    csv_path = csv_path or (manifest_path.with_name(manifest_path.name[:-len(".matrix.json")] + ".csv")
                            if manifest_path.name.endswith(".matrix.json") else manifest_path.with_suffix(".csv"))
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    with csv_path.open(encoding="utf-8", newline="") as stream:
        reader = csv.reader(stream, strict=True)
        header = next(reader, [])
        records = []
        for line, values in enumerate(reader, 2):
            if len(values) != len(header):
                raise ValueError(f"Matrix CSV line {line} has {len(values)} fields; expected {len(header)}")
            records.append(dict(zip(header, values)))
    return manifest, header, records


def validate_data(manifest, header, records):
    errors = []
    if header[:len(COLUMNS)] != COLUMNS or len(header) != len(set(header)) or any(not h.strip() for h in header):
        return ["Matrix CSV requires canonical core columns in order, followed by uniquely named extension columns"]
    if not isinstance(manifest, dict):
        return ["Matrix manifest must be a JSON object"]
    for field in ("matrix_id", "anchor_ref", "default_view"):
        if not isinstance(manifest.get(field), str) or not manifest[field].strip():
            errors.append(f"Matrix manifest requires nonempty {field}")
    if manifest.get("protocol") != PROTOCOL:
        errors.append(f"Matrix manifest protocol must be {PROTOCOL}")
    views = manifest.get("views")
    if not isinstance(views, list) or not views:
        errors.append("Matrix manifest requires nonempty views")
        views = []
    addresses = {}
    for view in views:
        if not isinstance(view, dict):
            errors.append("Matrix view must be an object")
            continue
        key = view.get("id")
        if not isinstance(key, str) or not key.strip():
            errors.append("Matrix view requires nonempty id")
            continue
        if key in addresses:
            errors.append(f"Duplicate matrix view id: {key}")
        for field in ("title", "semantics"):
            if not isinstance(view.get(field), str) or not view[field].strip():
                errors.append(f"Matrix view {key} requires nonempty {field}")
        axes = []
        for field in ("row_axis", "column_axis"):
            axis = view.get(field)
            if not isinstance(axis, dict):
                errors.append(f"Matrix view {key} requires {field} object")
                axis = {}
            for required in ("id", "label"):
                if not isinstance(axis.get(required), str) or not axis[required].strip():
                    errors.append(f"Matrix view {key} {field} requires nonempty {required}")
            members = axis.get("members")
            if not isinstance(members, list) or not members:
                errors.append(f"Matrix view {key} {field} requires nonempty members")
                members = []
            ids = []
            for member in members:
                if not isinstance(member, dict) or any(not isinstance(member.get(f), str) or not member[f].strip() for f in ("id", "label")):
                    errors.append(f"Matrix view {key} {field} member requires nonempty id and label")
                    continue
                ids.append(member["id"])
                if "ql_coordinate" in member:
                    coordinate = member["ql_coordinate"]
                    if (not isinstance(coordinate, dict)
                            or type(coordinate.get("position")) is not int
                            or coordinate["position"] not in range(6)
                            or coordinate.get("face") not in ("direct", "conjugate")):
                        errors.append(f"Matrix view {key} member {member['id']} has invalid ql_coordinate")
                if "source_ref" in member and (not isinstance(member["source_ref"], str) or not member["source_ref"].strip()):
                    errors.append(f"Matrix view {key} member {member['id']} requires nonempty source_ref when supplied")
            if len(ids) != len(set(ids)):
                errors.append(f"Matrix view {key} {field} has duplicate members")
            axes.append(set(ids))
        addresses[key] = axes
        for reference in ("shape_ref", "lens_ref", "derivation_ref"):
            if reference in view and (not isinstance(view[reference], str) or not view[reference].strip()):
                errors.append(f"Matrix view {key} {reference} must be nonempty text")
        shape = view.get("shape_ref")
        if isinstance(shape, str):
            if shape == "ql:shape:1.0.0:6x6:direct-conjugate":
                if not isinstance(view.get("derivation_ref"), str) or not view["derivation_ref"].strip():
                    errors.append(f"Matrix view {key} native 6x6 shape requires derivation_ref")
                for field, face in (("row_axis", "direct"), ("column_axis", "conjugate")):
                    axis = view.get(field)
                    members = axis.get("members", []) if isinstance(axis, dict) else []
                    if not isinstance(members, list):
                        members = []
                    positions = []
                    for member in members:
                        if not isinstance(member, dict):
                            continue
                        coordinate = member.get("ql_coordinate")
                        if (not isinstance(coordinate, dict)
                                or type(coordinate.get("position")) is not int
                                or coordinate["position"] not in range(6)
                                or coordinate.get("face") != face):
                            errors.append(f"Matrix view {key} native 6x6 shape requires explicit {face} ql_coordinate positions 0 through 5")
                        else:
                            positions.append(coordinate["position"])
                        if not isinstance(member.get("source_ref"), str) or not member["source_ref"].strip():
                            errors.append(f"Matrix view {key} native 6x6 member requires source_ref provenance")
                    if sorted(positions) != list(range(6)):
                        errors.append(f"Matrix view {key} native 6x6 shape requires complete unique {face} positions 0 through 5")

    if not isinstance(manifest.get("default_view"), str) or manifest["default_view"] not in addresses:
        errors.append("Matrix default_view must identify a declared view")
    ids = set()
    capability_ids = {r.get("id") for r in records if r.get("record_type") == "capability"}
    for record in records:
        key = record.get("id", "")
        if not key.strip():
            errors.append("Matrix record requires nonempty id")
        if key in ids:
            errors.append(f"Duplicate matrix id: {key}")
        ids.add(key)
        try:
            refs = json.loads(record["capability_refs"])
            if not isinstance(refs, list) or any(not isinstance(ref, str) or not ref.strip() for ref in refs) or len(refs) != len(set(refs)):
                raise ValueError("expected array of unique nonempty capability ids")
        except (ValueError, TypeError) as exc:
            errors.append(f"Matrix row {key} invalid capability_refs JSON: {exc}")
            refs = []
        try:
            extensions = json.loads(record["extensions"])
            if not isinstance(extensions, dict):
                raise ValueError("expected object")
        except (ValueError, TypeError) as exc:
            errors.append(f"Matrix row {key} invalid extensions JSON: {exc}")
            extensions = {}
        for ref in refs:
            if ref not in capability_ids:
                errors.append(f"Matrix row {key} has unknown capability reference: {ref}")
        if record["standing"] not in STANDINGS:
            errors.append(f"Matrix row {key} has unknown standing: {record['standing']}")
        kind = record["record_type"]
        if kind == "capability":
            if any(record[f] for f in ("view_id", "row_id", "column_id")) or refs:
                errors.append(f"Capability {key} cannot carry a view address or capability_refs")
            for field in ("need", "operation", "outcome", "implementation_status", "source_refs", "account_ref"):
                if not record[field].strip():
                    errors.append(f"Capability {key} requires nonempty {field}")
        elif kind == "relation":
            axes = addresses.get(record["view_id"])
            if axes is None:
                errors.append(f"Matrix row {key} has unknown view: {record['view_id']}")
            else:
                for field, axis in zip(("row_id", "column_id"), axes):
                    if record[field] not in axis:
                        errors.append(f"Matrix row {key} has unknown {field} member: {record[field]}")
            if not record["relation"].strip():
                errors.append(f"Matrix relation {key} requires concrete relation text")
            if not record["source_refs"].strip() and not extensions.get("source_evidence"):
                errors.append(f"Matrix relation {key} requires source_refs or extensions.source_evidence")
        else:
            errors.append(f"Matrix row {key} has unknown record_type: {kind}")
    return errors


def validate(manifest_path: Path, csv_path: Path | None = None):
    try:
        return validate_data(*load(manifest_path, csv_path))
    except (OSError, UnicodeError, ValueError, csv.Error) as exc:
        return [f"Cannot read capability matrix: {exc}"]


def validate_product_profile(manifest, product_index, namespace):
    errors = []
    if manifest.get("default_view") != "product-field":
        errors.append("Product matrix default_view must be product-field")
    view = next((v for v in manifest.get("views", []) if isinstance(v, dict) and v.get("id") == "product-field"), {})
    rows = view.get("row_axis", {}).get("members", [])
    cols = view.get("column_axis", {}).get("members", [])
    if [m.get("id") for m in rows] != [f"q{i}" for i in range(6)]:
        errors.append("Product matrix rows must be ordered q0 through q5")
    if [m.get("source_ref") for m in rows] != [f"{namespace}:seed:q{i}" for i in range(6)]:
        errors.append("Product matrix rows must reference corresponding HTML seed answers")
    if [m.get("id") for m in cols] != ["S"] + [f"S{i}" for i in range(6) if i != product_index]:
        errors.append("Product matrix columns must be S followed by the five other products in order")
    return errors


def sync_csv_appendix(markdown: str, csv_text: str):
    """Replace only the single fenced CSV appendix; preserve every narrative byte."""
    pattern = re.compile(r"(^```csv[^\S\n]*\n).*?(^```[^\S\n]*$)", re.MULTILINE | re.DOTALL)
    if len(list(pattern.finditer(markdown))) != 1:
        raise ValueError("Matrix MD requires exactly one fenced csv appendix")
    return pattern.sub(lambda m: m[1] + csv_text.rstrip("\n") + "\n" + m[2], markdown)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--csv", type=Path)
    args = parser.parse_args()
    errors = validate(args.manifest, args.csv)
    for error in errors:
        print(f"ERROR: {error}")
    if not errors:
        print("Capability matrix: declared axes, addresses, references and records verified.")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
