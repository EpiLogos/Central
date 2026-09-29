#!/usr/bin/env python3
"""Central #242 joined proof: personal-history intake against a real ctrl.

Run: python3 tests/personal_history_joined.py --ctrl /built/ctrl
No substitute implements the owner operations. A missing binary is an error,
not a skip. Everything runs in disposable Worlds; nothing touches the live
ground or any private archive.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


def fnv(data: bytes) -> str:
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return f"central.content-fnv1a64/v1:{len(data)}:{value:016x}"


LONG_BODY = (
    ["---\ndate: 2026-05-20\ntype: essay\ntitle: The long walk\n---\n# The long walk\n\n"]
    + [
        f"Section {i}. The lane past the reservoir was flooded at the third stile, so I went "
        "the long way round by the quarry road, which adds two miles and one shepherd. The "
        "shepherd said the flock would not cross moving water before June, and that this was "
        "the sheep's opinion, not his.\n\n"
        for i in range(1, 900)
    ]
    + [
        "FINAL PARAGRAPH — and here is the decisive record: I decided on the quarry road, "
        "on 20 May 2026, to sell the van and keep the bicycle. Everything earlier in this "
        "entry is weather; this sentence is the decision.\n"
    ]
)


class Joined(unittest.TestCase):
    ctrl: str

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="central-personal-joined-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.root = self.base / "Central"
        (self.root / "Control/user").mkdir(parents=True)
        (self.root / "Work").mkdir()
        self.env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith(("CENTRAL_", "OI_", "AIKIT_", "BKMR_"))
        }
        self.env.update(
            HOME=str(self.base / "home"),
            CENTRAL_ROOT=str(self.root),
            CENTRAL_CTRL_BIN=self.ctrl,
            NO_COLOR="1",
        )
        (self.base / "home").mkdir()
        self.action("central.init")

    def run_cmd(self, args, *, success=True):
        result = subprocess.run(
            [str(v) for v in args],
            cwd=self.base,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=120,
        )
        if success:
            self.assertEqual(result.returncode, 0, (args, result.stdout, result.stderr))
        else:
            self.assertNotEqual(result.returncode, 0, (args, result.stdout, result.stderr))
        return result

    def action(self, op, values=None, *, success=True):
        values = values or {}
        result = self.run_cmd(
            [self.ctrl, "--json", "action", "run", op, json.dumps(values)],
            success=success,
        )
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError:
            self.fail(f"{op} did not return JSON: {result.stdout!r} {result.stderr!r}")
        if success:
            self.assertTrue(parsed.get("ok"), (op, parsed))
            return parsed.get("data", {})
        self.assertFalse(parsed.get("ok"), (op, parsed))
        return parsed

    def anchor_person(self, title="Test subject"):
        identity = self.root / "Control/user/identity"
        identity.mkdir(parents=True, exist_ok=True)
        (identity / "present.md").write_text("I keep field notes.\n")
        manifest = {
            "schema": "central.pasu.identity-manifest/v1",
            "revision": "1",
            "subject": {"ref": "central:pasu:nara:local", "title": title},
            "identity_source": {
                "path": "Control/user/identity",
                "provenance_law": "test corpus",
                "sources": [
                    {"path": "Control/user/identity/present.md", "standing": "authored-ground"}
                ],
            },
        }
        (identity / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")

    def write_collection(self, origin: Path):
        (origin / "journal").mkdir(parents=True, exist_ok=True)
        (origin / "attachments").mkdir(exist_ok=True)
        (origin / "README.md").write_text("# Field notes (controlled)\n")
        (origin / "journal/2026-03-14-first-thaw.md").write_text(
            "---\ndate: 2026-03-14\ntype: journal\nweather: sleet\n---\n"
            "# First thaw\n\nSnowdrops — स्नोड्रॉप्स 🌱 — by the back step.\n"
            '> "We shall not cease from exploration" — quoted, not mine.\n'
        )
        (origin / "journal/2026-04-02-dream.md").write_text(
            "---\ndate: 2026-04-02\ntype: dream\n---\n# The two doors\n\nA mood with furniture.\n"
        )
        (origin / "journal/2026-05-20-the-long-walk.md").write_text("".join(LONG_BODY))
        (origin / "journal/2026-06-11-correction.md").write_text(
            "---\ndate: 2026-06-11\ntype: journal\ncorrects: 2026-03-14-first-thaw\n---\n"
            "# Correction\n\nThe snowdrops were crocuses.\n"
        )
        (origin / "attachments/scan.bin").write_bytes(b"\x00\x01\xff\xfe not text")

    def plan(self, origin: Path, **values):
        return self.action(
            "central.personal.collection.plan",
            {"path": str(origin), **values},
        )

    def apply(self, plan, *, acceptance="human-accepted", success=True):
        return self.action(
            "central.personal.collection.apply",
            {"plan": plan, "acceptance": acceptance},
            success=success,
        )

    # ------------------------------------------------------------------
    # A — same adoption machinery, both scopes, correct destinations
    # ------------------------------------------------------------------

    def test_a_same_machinery_copy_and_project_scope(self):
        self.anchor_person()
        origin = self.base / "archive"
        self.write_collection(origin)

        # Personal collection: copy into the person's chosen home.
        plan = self.plan(origin, collection_id="field-notes", title="Field notes")
        self.assertEqual(plan["placement"]["mode"], "copy")
        self.assertEqual(plan["person_ref"], "central:pasu:nara:local")
        outcome = self.apply(plan)
        self.assertEqual(outcome["receipt"]["entries_added"], 6)

        home = self.root / "Control/user/collections/field-notes"
        copied = (home / "journal/2026-05-20-the-long-walk.md").read_text()
        self.assertEqual(copied, "".join(LONG_BODY))
        self.assertIn("स्नोड्रॉप्स", (home / "journal/2026-03-14-first-thaw.md").read_text())

        record = json.loads((home / "collection.json").read_text())
        self.assertEqual(record["schema"], "central.personal-collection/v1")
        self.assertEqual(record["person_ref"], "central:pasu:nara:local")
        dispositions = sorted(entry["disposition"] for entry in record["entries"])
        self.assertEqual(dispositions.count("retained"), 5)
        self.assertEqual(dispositions.count("unreadable"), 1)

        # An ordinary Project: the same actions, project scope, retain-in-place.
        project = self.root / "Work/Notes"
        (project / "research").mkdir(parents=True)
        (project / "research/note.md").write_text("---\ndate: 2025-11-02\ntype: note\n---\nLab.\n")
        self.action("projectcentral.init", {"project": "Notes", "project_id": "Notes"})
        pplan = self.plan(
            self.root / "Work/Notes/research",
            project="Notes",
            collection_id="lab-notes",
        )
        self.assertEqual(pplan["world_ref"], "project:Notes")
        self.assertEqual(pplan["placement"]["mode"], "retain-in-place")
        self.assertTrue(
            pplan["entries"][0]["source_ref"].startswith("central:source:project:Notes:")
        )
        self.apply(pplan)
        listing = self.action("central.personal.collection.list")
        worlds = {c["collection_id"]: c.get("world_ref") for c in listing["collections"]}
        self.assertEqual(worlds["field-notes"], "control:root")
        self.assertEqual(worlds["lab-notes"], "project:Notes")

    # ------------------------------------------------------------------
    # B — long entry, decisive final paragraph, non-ASCII, unreadable member
    # ------------------------------------------------------------------

    def test_b_complete_original_survives_and_stays_readable(self):
        self.anchor_person()
        origin = self.base / "archive"
        self.write_collection(origin)
        plan = self.plan(origin, collection_id="field-notes")
        self.apply(plan)

        home = self.root / "Control/user/collections/field-notes"
        long_entry = home / "journal/2026-05-20-the-long-walk.md"
        text = long_entry.read_text()
        self.assertIn("FINAL PARAGRAPH", text[-400:], "the decisive final paragraph survived")
        self.assertGreater(long_entry.stat().st_size, 30_000)

        # Exact readback through the source identity, not just the file.
        source_ref = None
        record = json.loads((home / "collection.json").read_text())
        for entry in record["entries"]:
            if entry["entry_id"] == "journal/2026-05-20-the-long-walk.md":
                source_ref = entry["source_ref"]
        self.assertIsNotNone(source_ref)
        reading = self.action(
            "central.file-map.resolve", {"source_ref": source_ref, "content": True}
        )
        self.assertEqual(reading["result"]["content"], "".join(LONG_BODY))

        # The unreadable member remains visible as incomplete.
        verify = self.action("central.personal.collection.verify", {"collection_id": "field-notes"})
        self.assertEqual(verify["entries_total"], 6)
        unreadable = [
            entry
            for entry in record["entries"]
            if entry["disposition"] == "unreadable"
        ]
        self.assertEqual(len(unreadable), 1)
        self.assertIn("attachments/scan.bin", unreadable[0]["entry_id"])

    # ------------------------------------------------------------------
    # C — repeat/overlap imports, changed entries, interruption, rollback
    # ------------------------------------------------------------------

    def test_c_repeat_overlap_and_rollback(self):
        self.anchor_person()
        origin = self.base / "archive"
        self.write_collection(origin)
        plan = self.plan(origin, collection_id="field-notes")
        self.apply(plan)

        # Identical re-export: nothing to apply.
        same = self.plan(origin, collection_id="field-notes")
        unchanged = [entry for entry in same["entries"] if entry["action"] == "none"]
        self.assertEqual(len(unchanged), 6)
        self.apply(same, success=False)

        # Overlapping export: one changed entry, one new, one removed at origin.
        (origin / "journal/2026-03-14-first-thaw.md").write_text(
            "---\ndate: 2026-03-14\ntype: journal\n---\n# First thaw (revised)\n\nMore text.\n"
        )
        (origin / "journal/2026-07-01-late.md").write_text(
            "---\ndate: 2026-07-01\ntype: poem\n---\n# Late\n\nNew arrival.\n"
        )
        (origin / "attachments/scan.bin").unlink()
        overlap = self.plan(origin, collection_id="field-notes")
        actions = {e["entry_id"]: e["action"] for e in overlap["entries"]}
        self.assertEqual(actions["journal/2026-03-14-first-thaw.md"], "update")
        self.assertEqual(actions["journal/2026-07-01-late.md"], "copy-register")
        self.assertEqual(overlap["origin_absent"], ["attachments/scan.bin"])
        outcome = self.apply(overlap)
        self.assertEqual(outcome["receipt"]["entries_changed"], 1)
        self.assertEqual(outcome["receipt"]["entries_added"], 1)

        # Rollback of the second import restores the prior revision and
        # removes the added copy; the record stays for the first import.
        verify = self.action("central.personal.collection.verify", {"collection_id": "field-notes"})
        report = self.action(
            "central.personal.collection.rollback",
            {
                "collection_id": "field-notes",
                "import_sequence": 2,
                "expected_record_revision": verify["record_revision"],
            },
        )
        self.assertEqual(report["restored_entries"], ["journal/2026-03-14-first-thaw.md"])
        self.assertEqual(report["removed_entries"], ["journal/2026-07-01-late.md"])
        self.assertIn("(revised)", (origin / "journal/2026-03-14-first-thaw.md").read_text())
        self.assertNotIn(
            "(revised)",
            (
                self.root
                / "Control/user/collections/field-notes/journal/2026-03-14-first-thaw.md"
            ).read_text(),
        )

        # A later human edit is preserved by a rollback, never reverted.
        # (The rolled-back sequence number is free again; a re-import of the
        # still-present later export legitimately reuses the next number.)
        self.write_collection(origin)
        plan3 = self.plan(origin, collection_id="field-notes")
        outcome3 = self.apply(plan3)
        sequence = outcome3["receipt"]["sequence"]
        retained = (
            self.root
            / "Control/user/collections/field-notes/journal/2026-04-02-dream.md"
        )
        retained.write_text(retained.read_text() + "\nHand-added afterwards.\n")
        verify = self.action("central.personal.collection.verify", {"collection_id": "field-notes"})
        report = self.action(
            "central.personal.collection.rollback",
            {
                "collection_id": "field-notes",
                "import_sequence": sequence,
                "expected_record_revision": verify["record_revision"],
            },
        )
        self.assertEqual(report["preserved_entries"], ["journal/2026-04-02-dream.md"])
        self.assertIn("Hand-added afterwards.", retained.read_text())

    # ------------------------------------------------------------------
    # G — wrong person, no acceptance, stale revision, changed origin
    # ------------------------------------------------------------------

    def test_g_boundaries_refuse(self):
        origin = self.base / "archive"
        self.write_collection(origin)

        # No person anchor: the plan says so instead of inventing one.
        self.action("central.personal.collection.plan", {"path": str(origin)}, success=False)
        self.anchor_person(title="One person")

        wrong = self.action(
            "central.personal.collection.plan",
            {"path": str(origin), "person_ref": "central:pasu:nara:someone-else"},
            success=False,
        )
        self.assertIn("anchored subject", wrong["error"]["message"])

        plan = self.plan(origin, collection_id="field-notes")
        no_acceptance = self.apply(plan, acceptance="agent-suggested", success=False)
        self.assertIn("human-accepted", no_acceptance["error"]["message"])

        forged = dict(plan)
        forged["plan_revision"] = fnv(b"tampered")
        tampered = self.apply(forged, success=False)
        self.assertIn("identity", tampered["error"]["message"])

        self.apply(plan)
        # Origin moved after the plan: apply refuses and writes nothing new.
        (origin / "journal/2026-04-02-dream.md").write_text("changed mid-flight\n")
        plan2 = self.plan(origin, collection_id="field-notes")
        (origin / "journal/2026-04-02-dream.md").write_text("changed again\n")
        run = self.run_cmd(
            [
                self.ctrl,
                "--json",
                "action",
                "run",
                "central.personal.collection.apply",
                json.dumps({"plan": plan2, "acceptance": "human-accepted"}),
            ],
            success=False,
        )
        self.assertEqual(json.loads(run.stdout)["status"], "partial_completion")

    def test_h_apply_is_resumable_after_interruption(self):
        self.anchor_person()
        origin = self.base / "archive"
        self.write_collection(origin)
        plan = self.plan(origin, collection_id="field-notes")

        # Simulate an interrupted apply: the first two entries landed and
        # their journal steps persisted; the rest never ran.
        home = self.root / "Control/user/collections/field-notes"
        area = self.root / ".central/personal-history/field-notes"
        area.mkdir(parents=True)
        applied_ids = [
            "README.md",
            "journal/2026-03-14-first-thaw.md",
        ]
        for entry_id in applied_ids:
            destination = home / entry_id
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text((origin / entry_id).read_text())
        journal = {
            "schema": "central.personal-collection-journal/v1",
            "collection_id": "field-notes",
            "plan_revision": plan["plan_revision"],
            "sequence": 1,
            "started_at_unix_seconds": int(time.time()),
            "steps": [
                {"entry_id": entry_id, "kind": "place", "status": "applied"}
                for entry_id in applied_ids
            ],
        }
        (area / "journal.json").write_text(json.dumps(journal, indent=1) + "\n")

        status = self.action("central.personal.collection.status", {"collection_id": "field-notes"})
        self.assertIsNotNone(status["resumable_apply"])
        outcome = self.apply(plan)
        self.assertEqual(outcome["receipt"]["entries_added"], 6)
        verify = self.action("central.personal.collection.verify", {"collection_id": "field-notes"})
        self.assertEqual(verify["verified"], 6)
        self.assertEqual(verify["entries_total"], 6)
        # The resumed run absorbed the already-copied entries instead of
        # duplicating or refusing them.
        self.assertEqual(len(list((home / "journal").glob("*.md"))), 4)

    def test_refused_members_report_partial_completion(self):
        self.anchor_person()
        origin = self.base / "archive"
        self.write_collection(origin)
        plan = self.plan(origin, collection_id="field-notes")
        self.apply(plan)

        # A later export changes one entry; the origin changes again after
        # planning: that member is refused while the import stays honest.
        (origin / "journal/2026-04-02-dream.md").write_text(
            "---\ndate: 2026-04-02\ntype: dream\n---\n# Revised\n\nMore.\n"
        )
        overlap = self.plan(origin, collection_id="field-notes")
        (origin / "journal/2026-04-02-dream.md").write_text("changed mid-flight\n")
        result = self.run_cmd(
            [
                self.ctrl,
                "--json",
                "action",
                "run",
                "central.personal.collection.apply",
                json.dumps({"plan": overlap, "acceptance": "human-accepted"}),
            ],
            success=False,
        )
        parsed = json.loads(result.stdout)
        self.assertEqual(parsed["status"], "partial_completion")
        self.assertTrue(
            parsed["data"]["refused"][0].startswith("journal/2026-04-02-dream.md")
        )
        # The destination still holds the previously imported bytes.
        retained = (
            self.root
            / "Control/user/collections/field-notes/journal/2026-04-02-dream.md"
        )
        self.assertIn("furniture", retained.read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl", required=True, help="path to a source-built ctrl")
    args, rest = parser.parse_known_args()
    Joined.ctrl = args.ctrl
    unittest.main(argv=[sys_arg0(), *rest])


def sys_arg0():
    import sys

    return sys.argv[0]


if __name__ == "__main__":
    main()
