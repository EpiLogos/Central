"""Run maintenance checks against the native CLI and isolated authored copies."""
import csv
import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location("product_maintenance", ROOT / "tools/product_maintenance.py")
maintenance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(maintenance)


class ProductMaintenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.mirror = Path(self.temp.name)
        # Preserve absolute path depth because documented references deliberately
        # cross Central repositories. This is copied evidence, never a mock.
        self.root = (self.mirror / ROOT.relative_to(ROOT.anchor)).resolve()
        user = ROOT / "ProjectCentral/user"
        target = self.root / "ProjectCentral/user"
        target.mkdir(parents=True)
        for name in ("central.html", "capability-matrix.csv", "capability-matrix.json", "capability-matrix.md"):
            shutil.copy2(user / name, target / name)
        import check_product_ground
        account = check_product_ground.read_account(user / "central.html")
        refs = [(user, entry["href"]) for entry in json.loads(account.scripts["ground-resources"]).values()]
        refs.extend((user, href) for href in account.hrefs)
        with (user / "capability-matrix.csv").open(newline="") as stream:
            for row in csv.DictReader(stream):
                if row.get("record_type") == "capability":
                    for field in ("source_refs", "code_refs", "test_refs"):
                        refs.extend((ROOT, ref.strip()) for ref in row[field].split(";") if ref.strip())
        for base, reference in refs:
            parsed = urlsplit(reference)
            if not parsed.path or parsed.scheme:
                continue
            source = (base / unquote(parsed.path)).resolve()
            destination = self.mirror / source.relative_to(source.anchor)
            if source.is_file() and not destination.exists():
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
        # The real production binary is deliberately read-only in these tests.
        manifest_path = target / "capability-matrix.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["maintenance"]["cli"]["argv"][0] = str(ROOT / "target/debug/ctrl")
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        self.sync_md()
        # Record an actual reviewed basis after changing this isolated copy's
        # discovery executable. The maintenance gate must begin from settled
        # companion bytes, not from a synthetic provenance fixture.
        import reconcile_product_ground
        plan = reconcile_product_ground.make_plan(self.root, "central.html", "central", "html-to-csv")
        reconcile_product_ground.apply_plan(plan, plan["required_review"], "session:test-maintenance-baseline")

    @property
    def user(self):
        return self.root / "ProjectCentral/user"

    def sync_md(self):
        import capability_matrix
        csv_text = (self.user / "capability-matrix.csv").read_text()
        md = self.user / "capability-matrix.md"
        md.write_text(capability_matrix.sync_csv_appendix(md.read_text(), csv_text))

    def records(self):
        with (self.user / "capability-matrix.csv").open(newline="") as stream:
            reader = csv.DictReader(stream)
            return reader.fieldnames, list(reader)

    def write_records(self, header, rows):
        output = io.StringIO(newline="")
        writer = csv.DictWriter(output, fieldnames=header, lineterminator="\n")
        writer.writeheader(); writer.writerows(rows)
        (self.user / "capability-matrix.csv").write_text(output.getvalue())
        self.sync_md()

    def check(self, **kwargs):
        return maintenance.check(self.root, "central.html", "central", 0, **kwargs)

    @unittest.skipUnless((ROOT.parent / "Actuation/bin/actuation").is_file(), "Actuation is checked in the full suite workspace")
    def test_native_actuation_capability_table_has_the_real_commands(self):
        executable = ROOT.parents[0] / "Actuation/bin/actuation"
        completed = subprocess.run([str(executable), "capabilities", "--json"], text=True, capture_output=True, check=True)
        payload = json.loads(completed.stdout)
        self.assertTrue({"capabilities", "contract.list", "verify"} <= set(payload["commands"]))
        self.assertEqual(sorted(payload["commands"]), maintenance.discover(ROOT, {
            "argv": [str(executable), "capabilities", "--json"], "format": "json", "items_path": ["commands"]
        }))

    @unittest.skipUnless((ROOT.parent / "ai-kit/target/debug/aikit").is_file(), "AIKit is checked in the full suite workspace")
    def test_aikit_blank_description_help_entries_are_all_discovered_recursively(self):
        executable = ROOT.parents[0] / "ai-kit/target/debug/aikit"
        output = subprocess.run([str(executable), "knowledge", "--help"], text=True, capture_output=True, check=True).stdout
        immediate = maintenance.help_commands(output)
        self.assertEqual(
            ["search", "read", "relations", "route", "frame", "sources", "explain", "history", "status", "forget"],
            immediate,
        )
        discovered = maintenance.discover(ROOT, {"argv": [str(executable), "knowledge"], "format": "clap-help"})
        self.assertEqual(sorted({
            "search", "read", "relations", "route", "frame", "sources", "explain", "history", "status",
            "forget destination", "forget route", "forget project", "forget all",
        }), discovered)

    def test_current_central_cli_contract_and_repository_evidence_pass(self):
        errors, report = self.check(reference_scope="repository")
        self.assertEqual([], errors)
        actual = maintenance.discover(ROOT, {"argv": [str(ROOT / "target/debug/ctrl"), "action", "list", "--json"], "format": "json", "items_path": ["data", "actions"], "id_field": "id"})
        self.assertIn("central.init", actual)
        self.assertEqual(len(actual), report["discovered_commands"])
        self.assertEqual(len(actual), report["mapped_commands"])

    def test_readable_cli_catalogue_cannot_silently_lose_an_exposed_command(self):
        markdown = self.user / "capability-matrix.md"
        text = markdown.read_text()
        import re
        text, count = re.subn(r'^\| `central\.init`.*\n', '', text, count=1, flags=re.M)
        self.assertEqual(1, count)
        markdown.write_text(text)
        errors, _ = self.check(reference_scope="repository")
        self.assertIn("Readable CLI catalogue is stale; reconcile it from the capability records", errors)

    def test_missing_native_command_mapping_is_rejected(self):
        header, rows = self.records()
        row = next(row for row in rows if row["id"] == "cap.central.root")
        extension = json.loads(row["extensions"])
        extension["cli_commands"].remove("central.doctor")
        row["extensions"] = json.dumps(extension)
        self.write_records(header, rows)
        errors, _ = self.check()
        self.assertIn("Discoverable CLI command is unmapped: central.doctor", errors)

    def test_unknown_matrix_command_is_rejected(self):
        header, rows = self.records()
        row = next(row for row in rows if row["id"] == "cap.central.root")
        extension = json.loads(row["extensions"])
        extension["cli_commands"].append("central.nonexistent-action")
        row["extensions"] = json.dumps(extension)
        self.write_records(header, rows)
        errors, _ = self.check()
        self.assertIn("Matrix names an undiscoverable CLI command: central.nonexistent-action", errors)

    def test_changed_mapped_runtime_source_requires_reconciled_code_hash(self):
        source = self.root / "ctrl/src/root.rs"
        source.write_text(source.read_text() + "\n// real copied source changed after maintenance receipt\n")
        errors, _ = self.check()
        self.assertTrue(any(error.startswith("cap.central.root: code changed without reconciled capability evidence: ctrl/src/root.rs") for error in errors), errors)

    def test_new_runtime_source_in_real_git_comparison_requires_a_capability_mapping(self):
        subprocess.run(["git", "init"], cwd=self.root, check=True, capture_output=True, text=True)
        subprocess.run(["git", "config", "user.email", "maintenance-test@example.invalid"], cwd=self.root, check=True)
        subprocess.run(["git", "config", "user.name", "Maintenance test"], cwd=self.root, check=True)
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-m", "real copied ground baseline"], cwd=self.root, check=True, capture_output=True, text=True)
        base = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()
        new_source = self.root / "ctrl/src/unmapped_runtime.rs"
        new_source.write_text("// actual staged runtime source without a matrix capability\n")
        subprocess.run(["git", "add", str(new_source.relative_to(self.root))], cwd=self.root, check=True)
        errors, _ = self.check(base=base)
        self.assertIn("Changed runtime source has no capability mapping: ctrl/src/unmapped_runtime.rs", errors)


if __name__ == "__main__":
    unittest.main()
