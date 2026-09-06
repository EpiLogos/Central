"""Exercise reconciliation against copied, authored Central companions."""
import csv
import importlib.util
import io
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("reconcile_product_ground", ROOT / "tools/reconcile_product_ground.py")
reconcile = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reconcile)


class ReconcileProductGroundTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "central-copy"
        self.folder = self.root / "ProjectCentral/user"
        self.folder.mkdir(parents=True)
        self.account = "central.html"
        source = ROOT / "ProjectCentral/user"
        for name in (self.account, "capability-matrix.csv", "capability-matrix.json", "capability-matrix.md"):
            shutil.copy2(source / name, self.folder / name)
        # The checked-in pilot can carry an older provenance receipt. Establish
        # a real local baseline so each test proves its own review scope.
        baseline = self.plan("html-to-csv")
        reconcile.apply_plan(baseline, baseline["required_review"], "session:test-baseline")

    def paths(self):
        return reconcile.files(self.root, self.account)

    def bytes(self):
        return {name: path.read_bytes() for name, path in self.paths().items()}

    def plan(self, direction):
        return reconcile.make_plan(self.root, self.account, "central", direction)

    def apply(self, plan, reviewed=None):
        return reconcile.apply_plan(plan, plan["required_review"] if reviewed is None else reviewed, "session:real-reconciliation-test")

    def rewrite_html_seed(self, question, addition):
        path = self.folder / self.account
        text = path.read_text()
        text, count = re.subn(
            r'(<span data-seed-question="central:seed:q1">).*?(</span>)',
            lambda match: match[1] + question + match[2],
            text, count=1)
        self.assertEqual(count, 1)
        text, count = re.subn(
            r'(<div data-seed-content="central:seed:q1"><p>)(.*?)(</p>)',
            lambda match: match[1] + match[2] + addition + match[3], text, count=1, flags=re.S)
        self.assertEqual(count, 1)
        path.write_text(text)

    def set_csv_seed(self, text, question):
        csv_path = self.folder / "capability-matrix.csv"
        reader = csv.DictReader(io.StringIO(csv_path.read_text()))
        rows = list(reader)
        record = next(row for row in rows if row["record_type"] == "relation" and row["view_id"] == "product-field" and row["row_id"] == "q1" and row["column_id"] == "S")
        record["relation"] = text
        record["question"] = question
        output = io.StringIO(newline="")
        writer = csv.DictWriter(output, fieldnames=reader.fieldnames, lineterminator="\n")
        writer.writeheader(); writer.writerows(rows)
        csv_path.write_text(output.getvalue())

    def test_html_seed_and_question_propagate_to_all_ground_carriers(self):
        self.rewrite_html_seed("What must the owner decide now?", " This is the reviewed HTML seed change.")
        plan = self.plan("html-to-csv")
        self.assertIn("q1", plan["required_review"])
        self.assertEqual(["q1"], [item["row"] for item in plan["seed_changes"]])
        self.apply(plan)
        _, _, records = reconcile.matrix.load(self.folder / "capability-matrix.json")
        relation = next(row for row in records if row["view_id"] == "product-field" and row["row_id"] == "q1" and row["column_id"] == "S")
        self.assertIn("reviewed HTML seed change", relation["relation"])
        self.assertEqual("What must the owner decide now?", relation["question"])
        self.assertEqual("What must the owner decide now?", next(row for row in json.loads((self.folder / "capability-matrix.json").read_text())["views"] if row["id"] == "product-field")["row_axis"]["members"][1]["label"])
        html_text = (self.folder / self.account).read_text()
        self.assertIn('data-gloss="What must the owner decide now?"', html_text)
        self.assertIn('data-seed-sha256="' + reconcile.sha(relation["relation"].encode()) + '"', html_text)
        md = (self.folder / "capability-matrix.md").read_text()
        self.assertIn("What must the owner decide now?", md)
        self.assertIn("reviewed HTML seed change", md)

    def test_csv_seed_and_question_propagate_back_to_html(self):
        self.set_csv_seed("The CSV has the reviewed authoritative seed.", "Which owner can authorize it?")
        plan = self.plan("csv-to-html")
        self.assertIn("q1", plan["required_review"])
        self.apply(plan)
        html_text = (self.folder / self.account).read_text()
        self.assertIn("The CSV has the reviewed authoritative seed.", html_text)
        manifest = json.loads((self.folder / "capability-matrix.json").read_text())
        self.assertEqual("Which owner can authorize it?", manifest["views"][0]["row_axis"]["members"][1]["label"])
        self.assertIn('data-seed-question="central:seed:q1">Which owner can authorize it?</span>', html_text)
        self.assertIn('<span class="hero-label">Which owner can authorize it?</span>', html_text)
        self.assertIn('data-gloss="Which owner can authorize it?"', html_text)
        self.assertIn('data-seed-sha256="' + reconcile.sha(b"The CSV has the reviewed authoritative seed.") + '"', html_text)

    def test_descriptive_navigation_title_does_not_redefine_the_seed_question(self):
        path = self.folder / self.account
        text = path.read_text()
        text, count = re.subn(r'(<section class="ql-node"[^>]*data-resource="central:seed:q1"[^>]*>)', lambda m: re.sub(r'data-nav="[^"]*"', 'data-nav="The definition of this product"', m[1]), text, count=1)
        self.assertEqual(1, count)
        path.write_text(text)
        self.assertEqual([], self.plan("html-to-csv")["seed_changes"])

    def test_missing_expanded_review_rejects_without_writing_any_companion(self):
        self.rewrite_html_seed("What changed?", " Reviewed but not submitted.")
        plan = self.plan("html-to-csv")
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, "Expanded sections require review: q1"):
            self.apply(plan, reviewed=[])
        self.assertEqual(before, self.bytes())

    def test_stale_plan_refuses_a_concurrent_edit_without_writing(self):
        self.rewrite_html_seed("What changed?", " First real change.")
        plan = self.plan("html-to-csv")
        csv_path = self.folder / "capability-matrix.csv"
        csv_path.write_text(csv_path.read_text() + "\n")
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, "Stale plan: source changed; no files written"):
            self.apply(plan)
        self.assertEqual(before, self.bytes())

    def test_unknown_csv_column_and_extensions_survive_apply(self):
        csv_path = self.folder / "capability-matrix.csv"
        reader = csv.DictReader(io.StringIO(csv_path.read_text()))
        rows = list(reader); header = reader.fieldnames + ["future_owner_note"]
        relation = next(row for row in rows if row["view_id"] == "product-field" and row["row_id"] == "q1" and row["column_id"] == "S")
        relation["future_owner_note"] = "preserve this authored extension"
        ext = json.loads(relation["extensions"]); ext["future_extension"] = {"retained": True}; relation["extensions"] = json.dumps(ext)
        output = io.StringIO(newline=""); writer = csv.DictWriter(output, fieldnames=header, lineterminator="\n"); writer.writeheader(); writer.writerows(rows)
        csv_path.write_text(output.getvalue())
        self.rewrite_html_seed("What changed?", " Extension preservation test.")
        self.apply(self.plan("html-to-csv"))
        reader = csv.DictReader(io.StringIO(csv_path.read_text()))
        updated = next(row for row in reader if row["view_id"] == "product-field" and row["row_id"] == "q1" and row["column_id"] == "S")
        self.assertEqual("preserve this authored extension", updated["future_owner_note"])
        self.assertEqual({"retained": True}, json.loads(updated["extensions"])["future_extension"])

    def test_restore_restores_exact_original_bytes_and_rejects_later_work(self):
        self.rewrite_html_seed("What changed?", " Restore this exact transaction.")
        original = self.bytes()
        receipt = self.apply(self.plan("html-to-csv"))
        self.assertNotEqual(original, self.bytes())
        reconcile.restore(receipt)
        self.assertEqual(original, self.bytes())
        receipt = self.apply(self.plan("html-to-csv"))
        csv_path = self.folder / "capability-matrix.csv"
        csv_path.write_text(csv_path.read_text() + "# later authored work\n")
        current = self.bytes()
        with self.assertRaisesRegex(ValueError, "Restore would overwrite later work: capability-matrix.csv"):
            reconcile.restore(receipt)
        self.assertEqual(current, self.bytes())

    def test_relocated_real_receipt_cannot_target_the_project_ground(self):
        self.rewrite_html_seed("What changed?", " Receipt containment regression.")
        receipt = self.apply(self.plan("html-to-csv"))
        malicious = Path(self.temp.name) / "outside-project-journal"
        shutil.copytree(receipt.parent, malicious)
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, "Receipt must belong to this project documentation transaction journal"):
            reconcile.restore(malicious / "receipt.json")
        self.assertEqual(before, self.bytes())


if __name__ == "__main__":
    unittest.main()
