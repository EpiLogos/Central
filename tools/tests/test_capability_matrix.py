"""Validate and mutate real authored carriers; no mocked filesystem or parser."""
import csv
import io
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import capability_matrix as matrix


class MatrixTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "capability-matrix.json"
        for suffix in (".json", ".csv"):
            shutil.copy2(ROOT / "ProjectCentral/user" / ("capability-matrix" + suffix), self.path.with_suffix(suffix))
        self.manifest, self.header, self.records = matrix.load(self.path)

    def save(self):
        self.path.write_text(json.dumps(self.manifest), encoding="utf-8")
        with self.path.with_suffix(".csv").open("w", encoding="utf-8", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=self.header, lineterminator="\n")
            writer.writeheader()
            writer.writerows(self.records)

    def errors(self):
        self.save()
        return matrix.validate(self.path)

    def assert_error(self, text):
        errors = self.errors()
        self.assertTrue(any(text in error for error in errors), errors)

    def relation(self):
        return next(r for r in self.records if r["record_type"] == "relation")

    def test_real_central_carrier(self):
        self.assertEqual([], self.errors())

    def test_unknown_member(self):
        self.relation()["row_id"] = "absent-row"
        self.assert_error("unknown row_id member")

    def test_unknown_view(self):
        self.relation()["view_id"] = "absent-view"
        self.assert_error("unknown view")

    def test_missing_capability_reference(self):
        self.relation()["capability_refs"] = '["cap.central.absent"]'
        self.assert_error("unknown capability reference")

    def test_malformed_reference_json(self):
        self.relation()["capability_refs"] = "[broken"
        self.assert_error("invalid capability_refs JSON")

    def test_reference_json_requires_array(self):
        self.relation()["capability_refs"] = '{}'
        self.assert_error("invalid capability_refs JSON")

    def test_malformed_extension_json(self):
        self.relation()["extensions"] = "{broken"
        self.assert_error("invalid extensions JSON")

    def test_extension_json_requires_object(self):
        self.relation()["extensions"] = "[]"
        self.assert_error("invalid extensions JSON")

    def test_capability_cannot_be_cell_address(self):
        next(r for r in self.records if r["record_type"] == "capability")["row_id"] = "q0"
        self.assert_error("cannot carry a view address")

    def test_duplicate_ids_across_records(self):
        self.records[-1]["id"] = self.records[0]["id"]
        self.assert_error("Duplicate matrix id")

    def test_duplicate_members(self):
        view = self.manifest["views"][0]
        view["row_axis"]["members"].append(dict(view["row_axis"]["members"][0]))
        self.assert_error("duplicate members")

    def test_relational_reading_requires_source_basis(self):
        row = self.relation()
        row["source_refs"] = ""
        row["extensions"] = "{}"
        self.assert_error("requires source_refs or extensions.source_evidence")

    def test_cell_can_hold_multiple_relations(self):
        row = dict(self.relation())
        row["id"] += ".second-reading"
        self.records.append(row)
        self.assertEqual([], self.errors())

    def test_sparse_view_needs_no_synthetic_records(self):
        self.records = [r for r in self.records if r["record_type"] == "capability"]
        self.assertEqual([], self.errors())

    def test_extra_columns_and_unicode_survive_real_csv_roundtrip(self):
        self.header.append("review_note")
        for record in self.records:
            record["review_note"] = 'Human review: “why?”, then\ninspect the evidence.'
        self.assertEqual([], self.errors())
        manifest, header, records = matrix.load(self.path)
        self.assertEqual(self.manifest, manifest)
        self.assertEqual(self.header, header)
        self.assertEqual(self.records, records)

    def test_wrong_core_column_order(self):
        self.header[0], self.header[1] = self.header[1], self.header[0]
        self.assert_error("canonical core columns in order")

    def test_falsely_declared_native_shape(self):
        view = next(v for v in self.manifest["views"] if v["id"] == "product-field")
        view["shape_ref"] = "ql:shape:1.0.0:6x6:direct-conjugate"
        view["row_axis"]["id"] = "direct"
        view["column_axis"]["id"] = "conjugate"
        self.assert_error("native 6x6 shape requires")

    def test_supplied_ql_coordinate_requires_native_coordinate_types(self):
        member = self.manifest["views"][0]["row_axis"]["members"][0]
        for coordinate in (None, {"position": 9, "face": "direct"}, {"position": True, "face": "direct"}, {"position": 0, "face": "other"}, {"position": 0, "face": []}):
            with self.subTest(coordinate=coordinate):
                member["ql_coordinate"] = coordinate
                self.assert_error("invalid ql_coordinate")

    def test_optional_semantic_references_require_text_when_present(self):
        for field in ("shape_ref", "lens_ref", "derivation_ref"):
            with self.subTest(field=field):
                self.manifest["views"][0][field] = None
                self.assert_error(field + " must be nonempty text")
                del self.manifest["views"][0][field]

    def test_arbitrary_axis_size_uses_actual_members(self):
        # A bounded reading of the actual product field: preserve real members
        # and real relations, selecting two rows and three columns.
        view = next(v for v in self.manifest["views"] if v["id"] == "product-field")
        view["row_axis"]["members"] = view["row_axis"]["members"][:2]
        view["column_axis"]["members"] = view["column_axis"]["members"][:3]
        row_ids = {m["id"] for m in view["row_axis"]["members"]}
        col_ids = {m["id"] for m in view["column_axis"]["members"]}
        self.records = [r for r in self.records if r["view_id"] != view["id"] or (r["row_id"] in row_ids and r["column_id"] in col_ids)]
        self.assertEqual([], self.errors())
        self.assertTrue(matrix.validate_product_profile(self.manifest, 0, "central"))

    def test_appendix_sync_preserves_surrounding_authored_prose(self):
        original = (ROOT / "ProjectCentral/user/capability-matrix.md").read_text()
        before, rest = original.split("```csv", 1)
        _, after = rest.split("```", 1)
        csv_text = self.path.with_suffix(".csv").read_text()
        revised = matrix.sync_csv_appendix(original, csv_text)
        self.assertTrue(revised.startswith(before + "```csv"))
        self.assertTrue(revised.endswith("```" + after))
        self.assertIn(csv_text.rstrip("\n"), revised)


class QlSourceCarrierTests(unittest.TestCase):
    def test_native_shape_coordinates_from_actual_ql_contract(self):
        path = ROOT.parent / "Quaternal-Logic/fixtures/kernel/ql-shape-contract-v1.json"
        if not path.is_file():
            self.skipTest("QL native shape contract checkout unavailable")
        contract = json.loads(path.read_text())
        shape = contract["six_by_six"]
        view = {
            "id": "native-contract", "title": "Native QL shape address declaration",
            "semantics": "Structural addresses from the native contract; no capability assertion.",
            "shape_ref": shape["shape_ref"], "derivation_ref": str(path),
        }
        for axis, face in (("row_axis", shape["row_face"]), ("column_axis", shape["column_face"])):
            view[axis] = {"id": axis, "label": face, "members": [
                {"id": f"{face}:{position}", "label": f"{face} {position}",
                 "source_ref": str(path), "ql_coordinate": {"position": position, "face": face}}
                for position in shape["position_order"]]}
        manifest = {"protocol": matrix.PROTOCOL, "matrix_id": "native-contract",
                    "anchor_ref": contract["whole_anchor"], "default_view": view["id"], "views": [view]}
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "native.json"
            target.write_text(json.dumps(manifest))
            target.with_suffix(".csv").write_text(",".join(matrix.COLUMNS) + "\n")
            self.assertEqual([], matrix.validate(target))
            view["column_axis"]["members"][0]["ql_coordinate"]["face"] = "direct"
            target.write_text(json.dumps(manifest))
            self.assertTrue(any("explicit conjugate" in e for e in matrix.validate(target)))
            view["column_axis"]["members"][0]["ql_coordinate"]["face"] = "conjugate"
            view["row_axis"]["members"][0]["ql_coordinate"]["position"] = True
            target.write_text(json.dumps(manifest))
            self.assertTrue(any("explicit direct" in e for e in matrix.validate(target)))

    def test_existing_ql_mef_carriers_use_the_same_contract(self):
        directory = ROOT.parent / "Quaternal-Logic/docs/integrations/epi-logos"
        if not directory.is_dir():
            self.skipTest("QL source checkout is not available for cross-product integration")
        paths = sorted(directory.glob("*.matrix.json"))
        self.assertEqual(3, len(paths))
        sizes = set()
        for path in paths:
            with self.subTest(carrier=path.name):
                self.assertEqual([], matrix.validate(path))
                manifest, _, records = matrix.load(path)
                self.assertTrue(records)
                for view in manifest["views"]:
                    sizes.add((len(view["row_axis"]["members"]), len(view["column_axis"]["members"])))
        self.assertIn((6, 6), sizes)
        self.assertIn((12, 12), sizes)


if __name__ == "__main__":
    unittest.main()
