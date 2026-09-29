"""Inventory a real fixture documentation tree; no mocked filesystem or parser."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tools/tests/fixtures/documentation_field"
sys.path.insert(0, str(ROOT / "tools"))
import capability_matrix as matrix
import documentation_inventory as inventory


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.result = inventory.inventory([FIXTURE], FIXTURE)
        self.records = {r["source_ref"]: r for r in self.result["records"]}

    def test_schema_and_roles(self):
        self.assertEqual("central.documentation-inventory/v1", self.result["schema"])
        self.assertEqual({
            "vision.html": "vision", "design.md": "design", "mockup.html": "mockup",
            "architecture.md": "architecture", "diagrams/topology.mmd": "diagram",
            "diagrams/sequence.mmd": "diagram", "capability-matrix.csv": "capability-matrix",
        }, {k: v["role"] for k, v in self.records.items()})

    def test_standing_is_declared_or_defaults_to_agent_inference(self):
        self.assertEqual("authored-human-position", self.records["vision.html"]["standing"])
        self.assertEqual("design-commitment", self.records["design.md"]["standing"])
        self.assertEqual("architecture-contract", self.records["diagrams/topology.mmd"]["standing"])
        self.assertEqual("agent-inference", self.records["mockup.html"]["standing"])

    def test_unit_ids(self):
        self.assertIn("q2-failure", self.records["vision.html"]["unit_ids"])
        self.assertEqual(["ledger-design", "journeys", "state"], self.records["design.md"]["unit_ids"])
        self.assertEqual(["state-default", "state-failure"], self.records["mockup.html"]["unit_ids"])
        self.assertEqual(["comp_cli", "comp_store", "comp_reader"], self.records["diagrams/topology.mmd"]["unit_ids"])
        self.assertEqual(["actor_session", "comp_store"], self.records["diagrams/sequence.mmd"]["unit_ids"])
        self.assertEqual(["cap.ledger.record"], self.records["capability-matrix.csv"]["unit_ids"])

    def test_revision_is_content_hash(self):
        import hashlib
        data = (FIXTURE / "design.md").read_bytes()
        self.assertEqual("sha256:" + hashlib.sha256(data).hexdigest(), self.records["design.md"]["revision"])

    def test_records_are_bounded_and_carry_no_bodies(self):
        for key, record in self.records.items():
            with self.subTest(source=key):
                self.assertLessEqual(len(record["determination"]), inventory.MAX_DETERMINATION)
                self.assertLess(len(json.dumps(record)), 4096)
        self.assertNotIn("Readers never see a partial record", json.dumps(self.result))
        self.assertNotIn("The store owns every record", json.dumps(self.result))

    def test_determination_is_heading_and_first_sentence(self):
        self.assertEqual("Ledger design — Recording a return is one command that either succeeds or explains why it refused.",
                         self.records["design.md"]["determination"])

    def test_long_determination_is_truncated(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "design.md"
            path.write_text("# Long\n\n" + "word " * 200 + ".\n", encoding="utf-8")
            record = inventory.inventory([path], Path(temp))["records"][0]
            self.assertEqual(inventory.MAX_DETERMINATION, len(record["determination"]))

    def test_unknown_standing_and_role_fall_back(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "notes.md"
            path.write_text("---\nstanding: very-true\n---\n# Notes\n\nA note.\n", encoding="utf-8")
            record = inventory.inventory([Path(temp)], Path(temp))["records"][0]
            self.assertEqual(("other", "agent-inference"), (record["role"], record["standing"]))

    def test_reverse_traversal_from_capability_reaches_vision(self):
        self.assertEqual(["architecture.md", "design.md", "vision.html"],
                         inventory.upward(self.result["records"], "cap.ledger.record"))

    def test_reverse_traversal_discloses_missing_layer(self):
        with tempfile.TemporaryDirectory() as temp:
            copy = Path(temp) / "docs"
            shutil.copytree(FIXTURE, copy)
            (copy / "design.md").unlink()
            result = inventory.inventory([copy], copy)
            self.assertEqual(["architecture.md"], inventory.upward(result["records"], "cap.ledger.record"))
            self.assertEqual([], inventory.upward(result["records"], "cap.absent"))

    def test_fixture_matrix_documentation_extension_validates(self):
        self.assertEqual([], matrix.validate(FIXTURE / "capability-matrix.json"))

    def test_every_documentation_ref_in_fixture_resolves(self):
        records = self.result["records"]
        for record in records:
            for relation in record["relations"]:
                if relation["relation"] == "praxis_refs":
                    continue
                with self.subTest(source=record["source_ref"], target=relation["target"]):
                    target = inventory.resolve(records, record["source_ref"], relation["target"])
                    self.assertIsNotNone(target)
                    unit = relation["target"].partition("#")[2]
                    if unit:
                        self.assertIn(unit, target["unit_ids"])


if __name__ == "__main__":
    unittest.main()
