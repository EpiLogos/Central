"""Exercise the actual account and matrix, then mutate real isolated file copies."""
import csv
import io
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("check_product_ground", ROOT / "tools/check_product_ground.py")
ground = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ground)


class ProductGroundTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.mirror = Path(self.temp.name)
        self.root = self.mirror / ROOT.relative_to(ROOT.anchor)
        self.source_html = ROOT / "ProjectCentral/user/central.html"
        self.html = self.root / "ProjectCentral/user/central.html"
        self.html.parent.mkdir(parents=True)
        for name in ("central.html", "capability-matrix.md", "capability-matrix.csv", "capability-matrix.json"):
            shutil.copy2(self.source_html.parent / name, self.html.parent / name)
        # Mirror each real referenced file at its existing path depth. Cross-repo
        # relative sources remain relative, and broken paths cannot escape into
        # the live workspace. No generated stand-ins or patched filesystem APIs.
        resources = json.loads(ground.read_account(self.source_html).scripts["ground-resources"])
        references = [(self.source_html.parent, entry["href"]) for entry in resources.values()]
        references.extend((self.source_html.parent, href) for href in ground.read_account(self.source_html).hrefs)
        with (self.source_html.parent / "capability-matrix.csv").open(newline="") as stream:
            for row in csv.DictReader(stream):
                if row.get("record_type") == "capability":
                    for field in ("source_refs", "code_refs", "test_refs"):
                        references.extend((ROOT, ref.strip()) for ref in row[field].split(";"))
        for base, ref in references:
            parts = urlsplit(ref)
            if parts.path and not parts.scheme:
                original = (base / unquote(parts.path)).resolve()
                target = self.mirror / original.relative_to(original.anchor)
                if original.is_file() and not target.exists():
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(original, target)

    def rewrite(self, before, after):
        text = self.html.read_text(encoding="utf-8")
        self.assertIn(before, text)
        self.html.write_text(text.replace(before, after, 1), encoding="utf-8")

    def assert_error(self, message):
        errors = ground.validate(self.root)
        self.assertTrue(any(message in error for error in errors), errors)

    def mutate_capability(self, field, value, capability_id=None):
        path = self.html.parent / "capability-matrix.csv"
        original = path.read_text(encoding="utf-8")
        reader = csv.DictReader(io.StringIO(original))
        rows = list(reader)
        row = next(row for row in rows if row["record_type"] == "capability" and (capability_id is None or row["id"] == capability_id))
        row[field] = value
        output = io.StringIO(newline="")
        writer = csv.DictWriter(output, fieldnames=reader.fieldnames, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
        revised = output.getvalue()
        path.write_text(revised, encoding="utf-8")
        md = self.html.parent / "capability-matrix.md"
        text = md.read_text(encoding="utf-8")
        self.assertIn(original.rstrip("\n"), text)
        md.write_text(text.replace(original.rstrip("\n"), revised.rstrip("\n")), encoding="utf-8")

    def test_changed_seed_requires_expansion_review(self):
        import re
        text = self.html.read_text()
        text, count = re.subn(r'(<div data-seed-content="central:seed:q1"><p>)(.*?)</p>',
                             lambda match: match[1] + match[2] + " A newly changed intention.</p>", text, count=1)
        self.assertEqual(count, 1)
        self.html.write_text(text)
        self.assert_error("Seed changed: reconcile expanded q1")

    def test_expansion_cannot_reference_a_different_seed(self):
        self.rewrite('data-seed-ref="central:seed:q2"', 'data-seed-ref="central:seed:q1"')
        self.assert_error("Expanded q2 must reference its corresponding seed")

    def test_overview_anchor_is_required(self):
        self.rewrite('data-whole-anchor="0/1"', 'data-whole-anchor="1"')
        self.assert_error("Overview must declare the 0/1 whole anchor")

    def test_intent_only_cannot_become_implemented_without_traces(self):
        self.mutate_capability("implementation_status", "source-inspected",
                               "cap.central.guided-ground-authoring")
        self.assert_error("requires nonempty code_refs")
        self.assert_error("requires nonempty test_refs")

    def test_capability_requires_user_need(self):
        self.mutate_capability("need", " ")
        self.assert_error("requires nonempty need")

    def test_capability_requires_implementation_status(self):
        self.mutate_capability("implementation_status", "")
        self.assert_error("requires nonempty implementation_status")

    def test_capability_broken_source_trace_is_rejected(self):
        self.mutate_capability("source_refs", "docs/missing-functional-source.md")
        self.assert_error("has unresolved source_refs")

    def test_capability_broken_code_trace_is_rejected(self):
        self.mutate_capability("code_refs", "ctrl/src/missing-implementation.rs")
        self.assert_error("has unresolved code_refs")

    def test_capability_broken_test_trace_is_rejected(self):
        self.mutate_capability("test_refs", "ctrl/tests/missing-functional-test.rs")
        self.assert_error("has unresolved test_refs")

    def test_capability_broken_account_route_is_rejected(self):
        self.mutate_capability("account_ref", "central.html#q1/absent-capability")
        self.assert_error("requires a resolvable account_ref")

    def test_capability_missing_html_linkage_is_rejected(self):
        resources = json.loads(ground.read_account(self.html).scripts["ground-resources"])
        key = next(key for key in resources if key.startswith("cap.central."))
        # Mutate the actual resource map; provenance now also records capability IDs.
        import re
        text = self.html.read_text()
        match = re.search(r'(<script[^>]*id="ground-resources"[^>]*>)(.*?)(</script>)', text, re.S)
        resources["unregistered-capability"] = resources.pop(key)
        self.html.write_text(text[:match.start(2)] + json.dumps(resources) + text[match.end(2):])
        self.assert_error("requires HTML resource linkage")

    def test_capability_missing_md_anchor_is_rejected(self):
        resources = json.loads(ground.read_account(self.html).scripts["ground-resources"])
        key = next(key for key in resources if key.startswith("cap.central."))
        anchor = key.replace(".", "-")
        md = self.html.parent / "capability-matrix.md"
        text = md.read_text(encoding="utf-8")
        self.assertIn(anchor, text)
        md.write_text(text.replace(anchor, "absent-anchor"), encoding="utf-8")
        self.assert_error("requires matrix MD anchor")

    def test_real_pilot_is_valid(self):
        self.assertEqual([], ground.validate(ROOT))
        self.assertEqual([], ground.validate(self.root))

    def test_duplicate_node_identity_is_rejected(self):
        account = ground.read_account(self.html)
        first, second = account.nodes[:2]
        self.rewrite(f'id="{second[1]["id"]}"', f'id="{first[1]["id"]}"')
        self.assert_error("Duplicate HTML id")

    def test_duplicate_route_is_rejected(self):
        account = ground.read_account(self.html)
        first, second = account.nodes[:2]
        self.rewrite(f'data-route="{second[1]["data-route"]}"', f'data-route="{first[1]["data-route"]}"')
        self.assert_error("Duplicate QL route")

    def test_unknown_wikilink_is_rejected(self):
        self.rewrite("</body>", "<p>[[missing-ground-resource|Missing]]</p></body>")
        self.assert_error("Unknown wikilink resource: missing-ground-resource")

    def test_missing_source_file_is_rejected(self):
        resources = json.loads(ground.read_account(self.html).scripts["ground-resources"])
        entry = next(entry for entry in resources.values() if urlsplit(entry["href"]).path)
        self.rewrite(json.dumps(entry["href"]), '"missing-source-file.md"')
        self.assert_error("Unresolved resource")

    def test_missing_local_fragment_is_rejected(self):
        resources = json.loads(ground.read_account(self.html).scripts["ground-resources"])
        entry = next(entry for entry in resources.values() if entry["href"].startswith("#"))
        self.rewrite(json.dumps(entry["href"]), '"#q0/nonexistent-node"')
        self.assert_error("Unresolved fragment")

    def test_matrix_drift_is_rejected(self):
        csv = self.html.parent / "capability-matrix.csv"
        csv.write_text(csv.read_text(encoding="utf-8").replace("cap.central.", "cap.changed.", 1), encoding="utf-8")
        self.assert_error("Matrix MD/CSV drift")

    def test_product_axis_cannot_include_itself(self):
        path = self.html.parent / "capability-matrix.json"
        manifest = json.loads(path.read_text())
        view = next(v for v in manifest["views"] if v["id"] == "product-field")
        view["column_axis"]["members"][-1]["id"] = "S0"
        path.write_text(json.dumps(manifest))
        # Remove placements into the changed address so only the profile is
        # violated; generic matrix validity does not impose product semantics.
        csv_path = self.html.parent / "capability-matrix.csv"
        _, header, records = ground.capability_matrix.load(path, csv_path)
        for record in records:
            if record["view_id"] == "product-field" and record["column_id"] == "S5":
                record["column_id"] = "S0"
        self.write_records(header, records)
        self.assert_error("columns must be S followed by the five other products")

    def write_records(self, header, records):
        path = self.html.parent / "capability-matrix.csv"
        output = io.StringIO(newline="")
        writer = csv.DictWriter(output, fieldnames=header, lineterminator="\n")
        writer.writeheader()
        writer.writerows(records)
        revised = output.getvalue()
        path.write_text(revised)
        md = self.html.parent / "capability-matrix.md"
        md.write_text(ground.capability_matrix.sync_csv_appendix(md.read_text(), revised))

    def test_product_seed_axis_links_are_checked(self):
        path = self.html.parent / "capability-matrix.json"
        manifest = json.loads(path.read_text())
        view = next(v for v in manifest["views"] if v["id"] == "product-field")
        view["row_axis"]["members"][0]["source_ref"] = "central:seed:q1"
        path.write_text(json.dumps(manifest))
        self.assert_error("rows must reference corresponding HTML seed answers")

    def mutate_product_relation(self, field, value):
        path = self.html.parent / "capability-matrix.json"
        _, header, records = ground.capability_matrix.load(path)
        row = next(r for r in records if r["record_type"] == "relation" and r["view_id"] == "product-field")
        row[field] = value
        self.write_records(header, records)

    def test_product_relation_account_link_is_checked(self):
        self.mutate_product_relation("account_ref", "central.html#q0/absent")
        self.assert_error("requires a resolvable account_ref")

    def test_product_relation_source_link_is_checked(self):
        self.mutate_product_relation("source_refs", "docs/absent-source.md")
        self.assert_error("has unresolved source_refs")

    def test_product_relation_seed_must_match_row(self):
        self.mutate_product_relation("extensions", '{"seed_ref":"central:seed:absent"}')
        self.assert_error("seed_ref must match its manifest row")

    def test_product_relation_source_unit_must_exist(self):
        self.mutate_product_relation("extensions", '{"source_unit":"absent-unit"}')
        self.assert_error("source_unit must identify an actual HTML unit")

    def test_plain_html_link_is_checked(self):
        self.rewrite("</body>", '<a href="missing-manifest.json">Matrix</a></body>')
        self.assert_error("Unresolved HTML link: missing-manifest.json")

    def test_plain_html_link_fragment_is_checked(self):
        self.rewrite("</body>", '<a href="capability-matrix.md#absent-grid">Matrix</a></body>')
        self.assert_error("Unresolved HTML link fragment")

    def test_missing_tags_and_invalid_standing_are_rejected(self):
        attrs = ground.read_account(self.html).nodes[0][1]
        self.rewrite(f'data-tags="{attrs["data-tags"]}"', 'data-tags=" "')
        self.rewrite(f'data-standing="{attrs["data-standing"]}"', 'data-standing="ratified-by-filename"')
        self.assert_error("requires nonempty data-tags")
        self.assert_error("unknown standing")

    def test_template_content_is_rejected(self):
        self.rewrite("</body>", "<p>[Content]</p></body>")
        self.assert_error("Unreplaced template")


if __name__ == "__main__":
    unittest.main()
