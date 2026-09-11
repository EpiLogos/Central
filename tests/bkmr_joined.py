#!/usr/bin/env python3
"""Real ctrl + bkmr + AIKit contract proof, restricted to disposable Worlds.

Run: python3 tests/bkmr_joined.py --ctrl /built/ctrl --bkmr /installed/bkmr --aikit /built/aikit
No mocks implement the owner operations. Missing binaries are errors, not skips.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import tomllib
import unittest


def fnv(data: bytes) -> str:
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return f"central.content-fnv1a64/v1:{len(data)}:{value:016x}"


class Joined(unittest.TestCase):
    ctrl: str
    bkmr: str
    aikit: str

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="central-bkmr-joined-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.root = self.base / "Central"
        (self.root / "Control/user").mkdir(parents=True)
        (self.root / "Work").mkdir()
        self.home = self.base / "home"
        self.home.mkdir()
        self.aikit_home = self.base / "aikit"
        # Prevent inherited personal or network provider settings participating.
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("CENTRAL_", "OI_", "AIKIT_", "BKMR_"))}
        self.env.update(HOME=str(self.home), AIKIT_HOME=str(self.aikit_home),
                        CENTRAL_ROOT=str(self.root), CENTRAL_CTRL_BIN=self.ctrl,
                        CENTRAL_BKMR_BIN=self.bkmr, NO_COLOR="1")

    def run_cmd(self, args, *, success=True, env=None, cwd=None):
        result = subprocess.run([str(v) for v in args], cwd=cwd or self.base,
                                env=env or self.env, capture_output=True, text=True, timeout=90)
        if success:
            self.assertEqual(result.returncode, 0, (args, result.stdout, result.stderr))
        else:
            self.assertNotEqual(result.returncode, 0, (args, result.stdout, result.stderr))
        return result

    def action(self, op, project=None, *, success=True, env=None, **values):
        if project is not None:
            values["project"] = project
        result = self.run_cmd([self.ctrl, "--json", "--root", self.root, "action", "run",
                               f"central.file-map.{op}", json.dumps(values)], success=success, env=env)
        response = json.loads(result.stdout)
        self.assertEqual(response["ok"], success, response)
        if not success:
            return response
        data = response["data"]
        self.assertEqual(data["schema"], "central.file-map/v1")
        self.assertEqual(data["operation"], op)
        return data["result"]

    def write(self, path, text=b"quartz common source\n"):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode() if isinstance(text, str) else text)
        return path

    def project(self, name):
        p = self.root / "Work" / name
        (p / "ProjectCentral/user").mkdir(parents=True)
        self.write(f"Work/{name}/ProjectCentral/project.json", json.dumps({
            "schema": "central.project/v1", "project_id": name,
            "human_source": "ProjectCentral/user", "wiki": {"profile": "okf-wiki/v1",
            "source": "ProjectCentral/agents/wiki/wiki.json"}}))
        return p

    def register(self, path, project=None, **options):
        basis = self.action("inspect", project)["revision"]
        return self.action("register", project, path=str(path), expected_revision=basis, **options)["source_ref"]

    def locate(self, path, project=None):
        return self.action("locate", project, path=str(path))

    def link(self, source, path, project=None):
        return self.action("link", project, source_ref=source, path=path, owner="joined-test",
                           expected_revision=self.action("inspect", project)["revision"])

    def move(self, source, destination, **options):
        plan = self.action("move-plan", source_ref=source, destination=destination)
        self.action("move-apply", plan_id=plan["plan_id"], quiesced=True, **options)
        return plan

    def native(self, *args, db=None, **options):
        db = db or self.root / ".central/bkmr/index.db"
        return self.run_cmd([self.bkmr, "--db", db, "--no-color", *args], **options)

    def ai(self, *args, success=True, cwd=None, env=None):
        result = self.run_cmd([self.aikit, "--json", "-C", cwd or self.root, *args],
                              success=success, env=env)
        return json.loads(result.stdout)

    def db_digest(self, project=None):
        root = self.root if project is None else self.root / "Work" / project
        return hashlib.sha256((root / ".central/bkmr/index.db").read_bytes()).hexdigest()

    def skill(self):
        self.write("Control/user/skills/astronomy/SKILL.md", "---\nname: astronomy\ndescription: Read the stars carefully\n---\nPreserve source provenance.\n")
        self.write("Control/user/skills/astronomy/skill.json", json.dumps({
            "schema": "central.skill/v1", "name": "astronomy", "scope": "control-user",
            "standing": "active", "provenance": "human-authored"}))
        self.write("Control/user/skills/astronomy/assets/data.bin", bytes([0, 255, 5, 0]))
        script = self.write("Control/user/skills/astronomy/scripts/read.sh", "#!/bin/sh\nprintf safe\n")
        script.chmod(0o755)
        return self.register("Control/user/skills")

    def promoted(self):
        ref = self.skill()
        self.ai("source", "bind-central", "owner-skills", ref, "--root", str(self.root))
        self.ai("source", "sync", "owner-skills")
        self.ai("source", "promote", "owner-skills")
        return ref

    def test_01_restart_retains_native_records_and_read_does_not_write_database(self):
        self.write("Control/user/note.md")
        self.action("refresh")
        before = self.db_digest()
        for _ in range(2):
            result = self.action("search", query="quartz")
            self.assertEqual(len(result["hits"]), 1)
            ref = result["hits"][0]["source"]["ref"]
            self.assertIn("quartz", self.action("resolve", source_ref=ref, content=True)["content"])
        self.assertEqual(self.db_digest(), before)

    def test_02_root_federation_retains_owning_world_and_project_scope(self):
        self.project("alpha"); self.project("beta")
        for name in ["alpha", "beta"]:
            self.write(f"Work/{name}/ProjectCentral/user/note.md")
            self.action("refresh", name)
        self.assertEqual(len(self.action("search", "alpha", query="quartz")["hits"]), 1)
        hits = self.action("search", query="quartz", federated=True)["hits"]
        self.assertEqual({h["world_ref"] for h in hits}, {"project:alpha", "project:beta"})
        self.assertTrue(self.action("inspect", federated=True)["provider"]["fulltext"])

    def test_03_incremental_refresh_preserves_unrelated_bookmark_and_user_metadata(self):
        note = self.write("Control/user/note.md")
        self.action("refresh")
        row = self.action("search", query="quartz")["hits"][0]
        self.native("add", "https://example.invalid/retained", "personal", "--title", "Hand made", "--no-web", "--no-embed")
        self.native("update", row["provider_binding"], "--title", "Chosen title", "--tags", "kept", "--no-embed")
        note.write_text("opal replacement\n")
        self.action("refresh")
        records = json.loads(self.native("search", "--json", "--np").stdout)
        self.assertEqual(len(records), 2)
        changed = next(r for r in records if str(r["id"]) == row["provider_binding"])
        self.assertEqual(changed["title"], "Chosen title")
        self.assertIn("kept", changed["tags"])
        self.assertEqual(len(self.action("search", query="opal")["hits"]), 1)

    def test_04_identical_file_bytes_keep_two_source_identities(self):
        self.write("Control/user/one.md"); self.write("Control/user/two.md")
        self.action("refresh")
        self.assertEqual(len({h["source"]["ref"] for h in self.action("search", query="quartz")["hits"]}), 2)

    def test_05_ordinary_file_directory_and_binary_need_no_frontmatter(self):
        file = self.write("outside/ordinary.txt", "plain untouched quartz")
        data = self.write("outside/picture.bin", bytes([0, 255, 4]))
        ref = self.register("outside/ordinary.txt")
        directory = self.register("outside")
        binary = self.register("outside/picture.bin")
        self.assertEqual(self.action("resolve", source_ref=directory)["kind"], "directory")
        returned = self.action("resolve", source_ref=binary, content=True, content_encoding="base64")
        self.assertEqual(base64.b64decode(returned["content"]), data.read_bytes())
        self.assertEqual(self.action("resolve", source_ref=ref, content=True)["content"], file.read_text())

    def test_06_native_import_uses_source_tracking_and_scope_base_path(self):
        path = self.write("Control/user/import.md", "---\nname: Native imported note\ntags: research,quartz\n---\nNative tracked quartz\n")
        before = path.read_bytes()
        ref = self.register("Control/user/import.md", native_import=True)
        self.action("refresh")
        index = json.loads((self.root / ".central/bkmr/bindings.json").read_text())
        record = index["entries"][ref]
        self.assertIsNotNone(record.get("import_id"), record)
        conn = sqlite3.connect(self.root / ".central/bkmr/index.db")
        try:
            columns = [x[1] for x in conn.execute("pragma table_info(bookmarks)")]
            self.assertIn("file_path", columns)
            row = conn.execute("select file_path from bookmarks where id=?", (record["import_id"],)).fetchone()
            self.assertTrue(row[0].startswith("$WORLD/"), row)
        finally:
            conn.close()
        self.assertEqual(path.read_bytes(), before)

    def test_07_privacy_revocation_withholds_stale_index_immediately(self):
        path = self.write("Control/user/private/note.md")
        ref = self.locate(path)["source"]["ref"]
        self.action("refresh")
        (path.parent / ".no-agent-retrieval").touch()
        self.assertEqual(self.action("search", query="quartz")["hits"], [])
        self.action("resolve", source_ref=ref, content=True, success=False)
        self.action("refresh")
        self.assertEqual(json.loads(self.native("search", "--json", "--np").stdout), [])

    def test_08_stale_source_is_not_served_as_current_index(self):
        path = self.write("Control/user/note.md")
        self.action("refresh")
        old = self.locate(path)
        path.write_text("new sapphire\n")
        result = self.action("search", query="quartz")
        self.assertEqual(result["hits"], [])
        self.assertTrue(result["absences"])
        self.action("resolve", source_ref=old["source"]["ref"], expected_revision=old["revision"], success=False)

    def test_09_root_link_keeps_project_source_ref(self):
        self.project("alpha")
        path = self.write("Work/alpha/ProjectCentral/user/note.md")
        ref = self.locate(path, "alpha")["source"]["ref"]
        self.link(ref, "Control/user/linked.md")
        self.assertEqual(self.locate("Control/user/linked.md")["source"]["ref"], ref)
        self.assertEqual(self.register(str(path), allow_external=True), ref)
        self.action("refresh", "alpha")
        self.assertEqual(self.action("search", query="quartz")["hits"][0]["source"]["ref"], ref)

    def test_10_link_refuses_foreign_replacement(self):
        self.write("Control/user/note.md")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        self.link(ref, "links/note")
        basis = self.action("inspect")["revision"]
        link = self.root / "links/note"
        link.unlink(); link.write_text("foreign retained")
        self.action("link", source_ref=ref, path="links/note", owner="joined-test",
                    expected_revision=basis, success=False)
        self.assertEqual(link.read_text(), "foreign retained")

    def test_11_cycle_and_unregistered_symlink_are_refused(self):
        self.write("plain/a.txt")
        ref = self.register("plain")
        self.action("link", source_ref=ref, path="plain/recurse", owner="joined-test",
                    expected_revision=self.action("inspect")["revision"], success=False)
        (self.root / "redirect").symlink_to(self.root / "plain")
        self.action("register", path="redirect/a.txt", expected_revision=self.action("inspect")["revision"], success=False)

    def test_12_external_resource_requires_explicit_registration(self):
        path = self.base / "external.txt"; path.write_text("external quartz")
        self.action("register", path=str(path), success=False)
        ref = self.register(str(path), allow_external=True)
        self.assertEqual(self.action("resolve", source_ref=ref, content=True)["content"], "external quartz")

    def test_13_move_source_repairs_link_preserves_identity_and_rolls_back(self):
        self.write("Control/user/note.md")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        self.link(ref, "links/note")
        self.action("refresh")
        plan = self.move(ref, "Control/user/renamed.md")
        self.assertEqual(self.locate("links/note")["source"]["ref"], ref)
        self.assertEqual(self.action("search", query="quartz")["hits"][0]["source"]["ref"], ref)
        self.action("move-rollback", plan_id=plan["plan_id"], quiesced=True)
        self.assertTrue((self.root / "Control/user/note.md").exists())
        self.assertEqual((self.root / "links/note").resolve(), self.root / "Control/user/note.md")

    def test_14_interrupted_move_replays_once(self):
        self.write("Control/user/note.md")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        self.link(ref, "links/note")
        plan = self.action("move-plan", source_ref=ref, destination="Control/user/renamed.md")
        env = dict(self.env, CENTRAL_FILE_MAP_TEST_INTERRUPT_AFTER_RENAME="1")
        self.action("move-apply", plan_id=plan["plan_id"], quiesced=True, env=env, success=False)
        for _ in range(2):
            self.action("move-apply", plan_id=plan["plan_id"], quiesced=True)
        self.assertEqual(self.locate("links/note")["source"]["ref"], ref)

    def test_15_rollback_never_overwrites_new_human_bytes(self):
        self.write("Control/user/note.md")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        plan = self.move(ref, "Control/user/renamed.md")
        changed = self.root / "Control/user/renamed.md"; changed.write_text("new human work")
        self.action("move-rollback", plan_id=plan["plan_id"], quiesced=True, success=False)
        self.assertEqual(changed.read_text(), "new human work")

    def test_16_move_refuses_occupied_destination_and_unquiesced_apply(self):
        self.write("Control/user/note.md"); self.write("Control/user/occupied.md", "other")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        self.action("move-plan", source_ref=ref, destination="Control/user/occupied.md", success=False)
        plan = self.action("move-plan", source_ref=ref, destination="Control/user/free.md")
        self.action("move-apply", plan_id=plan["plan_id"], success=False)
        self.assertTrue((self.root / "Control/user/note.md").exists())

    def test_17_whole_project_move_keeps_root_and_project_readings(self):
        self.project("alpha")
        path = self.write("Work/alpha/ProjectCentral/user/note.md")
        ref = self.locate(path, "alpha")["source"]["ref"]
        directory = self.register("Work/alpha")
        self.link(ref, "links/alpha")
        self.action("refresh", "alpha")
        plan = self.move(directory, "Work/renamed")
        self.assertEqual(self.locate("links/alpha")["source"]["ref"], ref)
        self.assertIn("quartz", self.action("resolve", source_ref=ref, content=True)["content"])
        self.assertEqual(self.action("search", query="quartz", federated=True)["hits"][0]["world_ref"], "project:alpha")
        self.action("move-rollback", plan_id=plan["plan_id"], quiesced=True)
        self.assertTrue(path.exists())

    def test_18_enclosing_root_relocation_rebinds_without_new_source_id(self):
        path = self.write("Control/user/note.md")
        ref = self.locate(path)["source"]["ref"]
        self.link(ref, "links/note"); self.action("refresh")
        moved = self.base / "MovedCentral"; self.root.rename(moved); self.root = moved
        self.env["CENTRAL_ROOT"] = str(moved)
        self.action("refresh")
        self.assertEqual(self.locate("links/note")["source"]["ref"], ref)
        self.assertEqual(self.action("search", query="quartz")["hits"][0]["source"]["ref"], ref)

    def test_19_external_project_scope_federates_and_stays_independent(self):
        p = self.project("external")
        self.write("Work/external/ProjectCentral/user/note.md")
        moved = self.base / "NativeProject"; p.rename(moved)
        self.action("scope-register", name="external", path=str(moved), allow_external=True,
                    expected_revision=self.action("inspect")["revision"])
        self.action("refresh", "external")
        self.assertEqual(len(self.action("search", query="quartz", federated=True)["hits"]), 1)
        result = self.run_cmd([self.ctrl,"--json","--root",moved,"action","run","central.file-map.search",'{"query":"quartz"}'])
        self.assertEqual(len(json.loads(result.stdout)["data"]["result"]["hits"]), 1)

    def test_20_database_adoption_keeps_bookmarks_and_independent_backup(self):
        source = self.base / "existing.db"
        self.native("create-db", str(source), db=source)
        self.native("add", "https://example.invalid/kept", "handmade", "--title", "Retained", "--description", "personal annotation", "--no-web", "--no-embed", db=source)
        before = source.read_bytes()
        receipt = self.action("adopt-db", database=str(source), quiesced=True)
        backup = self.root / receipt["backup"]
        saved = backup.read_bytes()
        self.write("Control/user/note.md"); self.action("refresh")
        self.assertEqual(source.read_bytes(), before)
        self.assertEqual(backup.read_bytes(), saved)
        self.assertEqual(len(json.loads(self.native("search","--json","--np").stdout)), 2)
        self.action("adopt-db", database=str(source), quiesced=True, success=False)

    def test_21_bkmr_absence_and_semantic_mode_are_explicit_without_creating_database(self):
        reading = self.action("inspect", env=dict(self.env, CENTRAL_BKMR_BIN=str(self.base / "missing")))
        self.assertFalse(reading["provider"]["available"])
        self.assertFalse((self.root / ".central/bkmr/index.db").exists())
        self.action("search", query="quartz", mode="semantic", success=False)

    def test_22_stale_ground_revision_and_native_row_drift_are_refused(self):
        self.write("plain/note.txt")
        initial = self.action("inspect")["revision"]
        ref = self.register("plain/note.txt")
        self.action("register", path="plain", expected_revision=initial, success=False)
        self.action("refresh")
        row = self.action("search", query="quartz")["hits"][0]
        self.native("update", row["provider_binding"], "--description", "authored correction quartz", "--no-embed")
        self.action("refresh", success=False)
        self.assertIn("authored correction", self.native("show",row["provider_binding"],"--json").stdout)

    def test_23_explicit_link_gives_one_foreign_source_not_whole_project(self):
        self.project("alpha"); self.project("beta")
        path = self.write("Work/beta/ProjectCentral/user/linked.md")
        self.write("Work/beta/ProjectCentral/user/unlinked.md")
        ref = self.locate(path, "beta")["source"]["ref"]
        self.link(ref, "ProjectCentral/user/from-beta.md", "alpha")
        self.action("refresh", "beta")
        hits = self.action("search", "alpha", query="quartz")["hits"]
        self.assertEqual([h["source"]["ref"] for h in hits], [ref])
        foreign = [e for e in self.action("inspect", "alpha")["resources"] if e["world_ref"]=="project:beta"]
        self.assertEqual([e["source"]["ref"] for e in foreign], [ref])

    def test_24_authored_world_exclusion_is_applied_to_linked_source(self):
        self.project("alpha")
        path = self.write("Control/user/note.md")
        ref = self.locate(path)["source"]["ref"]
        self.link(ref, "ProjectCentral/user/root-note.md", "alpha")
        self.action("refresh")
        for scope, project, record in [
            ("root", None, {"schema":"central.world-relations/v1","ref":"control:root","revision":"r1","parent":None,"sources":[{"ref":ref,"revision":"r1","authority":"human-authored","treatment":"retain-native"}]}),
            ("project", "alpha", {"schema":"central.world-relations/v1","ref":"alpha","revision":"r1","parent":"control:root","excluded_sources":[ref]})]:
            self.run_cmd([self.ctrl,"--json","--root",self.root,"action","run","central.world-relations.save",json.dumps({"scope":scope,"project":project,"record":record})])
        self.assertEqual(self.action("search", "alpha", query="quartz")["hits"], [])
        self.action("resolve", "alpha", source_ref=ref, content=True, success=False)

    def test_25_native_skill_tree_preserves_binary_modes_and_revision(self):
        ref = self.skill()
        tree = self.action("skill-tree", source_ref=ref)
        bypath = {f["path"]:f for f in tree["files"]}
        self.assertEqual(base64.b64decode(bypath["astronomy/assets/data.bin"]["content_base64"]), bytes([0,255,5,0]))
        self.assertEqual(bypath["astronomy/scripts/read.sh"]["mode"], 0o755)
        self.assertEqual(tree["tree_revision"], self.action("skill-tree", source_ref=ref)["tree_revision"])
        self.write("Control/user/skills/astronomy/assets/new.txt", "new companion")
        self.assertNotEqual(tree["tree_revision"], self.action("skill-tree", source_ref=ref)["tree_revision"])

    def test_26_skill_tree_refuses_symlink_and_private_subtree(self):
        ref = self.skill()
        bad = self.root / "Control/user/skills/astronomy/redirect"
        bad.symlink_to(self.root / "Control/user")
        self.action("skill-tree", source_ref=ref, success=False)
        bad.unlink()
        self.write("Control/user/skills/astronomy/assets/.no-agent-retrieval", b"")
        self.action("skill-tree", source_ref=ref, success=False)

    def test_27_actual_aikit_search_and_read_use_owner_without_database_write(self):
        self.write("Control/user/note.md")
        self.action("refresh")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        before = self.db_digest()
        search = self.ai("knowledge", "search", "quartz")
        self.assertIn(ref, json.dumps(search))
        read = self.ai("knowledge", "read", "source=" + ref)
        self.assertIn("quartz common source", json.dumps(read))
        self.assertEqual(before, self.db_digest())

    def test_28_aikit_owner_disconnect_never_returns_cached_source(self):
        self.write("Control/user/note.md"); self.action("refresh")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        self.ai("knowledge","search","quartz")
        self.ai("knowledge","read","source="+ref, success=False,
                env=dict(self.env,CENTRAL_CTRL_BIN=str(self.base / "missing-ctrl")))

    def test_29_aikit_root_search_works_with_only_a_project_database(self):
        self.project("alpha"); self.write("Work/alpha/ProjectCentral/user/note.md")
        self.action("refresh", "alpha")
        result = self.ai("knowledge", "search", "quartz")
        self.assertIn("project:alpha", json.dumps(result))
        self.assertFalse((self.root / ".central/bkmr/index.db").exists())

    def test_30_aikit_sync_and_promote_preserve_exact_companions(self):
        self.promoted()
        copies = list(self.aikit_home.rglob("data.bin"))
        self.assertTrue(copies)
        for copy in copies:
            self.assertEqual(copy.read_bytes(), bytes([0,255,5,0]))
        scripts = list(self.aikit_home.rglob("read.sh"))
        self.assertTrue(scripts)
        self.assertTrue(all(p.stat().st_mode & 0o100 for p in scripts))

    def test_31_aikit_retirement_invalidates_active_owner_snapshot(self):
        self.promoted()
        manifest = self.root / "Control/user/skills/astronomy/skill.json"
        value = json.loads(manifest.read_text()); value.update(standing="retired",retirement={"retired_by":"test human","retired_at_unix_seconds":1,"retirement_reason":"rest"})
        manifest.write_text(json.dumps(value))
        self.ai("status", success=False)
        self.ai("source", "sync", "owner-skills")
        self.ai("source", "promote", "owner-skills")

    def test_32_aikit_new_companion_requires_a_fresh_snapshot(self):
        self.promoted()
        self.write("Control/user/skills/astronomy/assets/new.txt", "additional")
        self.ai("status", success=False)

    def test_33_aikit_central_source_survives_enclosing_world_move(self):
        self.promoted()
        destination = self.base / "Relocated"; self.root.rename(destination); self.root=destination
        self.env["CENTRAL_ROOT"] = str(destination)
        self.ai("source", "sync", "owner-skills")
        self.ai("source", "promote", "owner-skills")

    def test_34_generation_records_actual_materialisation_not_loaded_harness(self):
        ref = self.promoted()
        status = self.ai("source", "show", "owner-skills")["data"]
        snapshot = self.aikit_home / "sources/owner-skills/snapshots" / status["active_snapshot"] / "snapshot.toml"
        capsule = tomllib.loads(snapshot.read_text())["skills"][0]["id"]
        self.ai("enable", capsule, "--scope", "global")
        self.ai("apply")
        report = self.action("projection-read", source_ref=ref)
        rows = report["records"]
        self.assertTrue(rows, report)
        self.assertTrue(all(row["kind"] == "reported-generation" for row in rows))
        self.assertTrue(all(not row["live_harness_claim"] for row in rows))
        self.assertTrue(all(row["source_current"] for row in rows))

    def test_35_unreadable_world_policy_never_widens_disclosure(self):
        self.write("Control/user/note.md"); self.action("refresh")
        self.write("Control/relations/worlds/broken.json", "not-json")
        result = self.action("search", query="quartz", success=False)
        self.assertNotIn("quartz common source", json.dumps(result))

    def test_36_concurrent_native_refreshes_preserve_one_record_per_source(self):
        self.write("Control/user/one.md"); self.write("Control/user/two.md")
        command = [self.ctrl,"--json","--root",str(self.root),"action","run","central.file-map.refresh","{}"]
        children = [subprocess.Popen(command,env=self.env,cwd=self.base,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for _ in range(4)]
        for child in children:
            out, err = child.communicate(timeout=90)
            self.assertEqual(child.returncode,0,(out,err))
            self.assertTrue(json.loads(out)["ok"])
        self.assertEqual(len(json.loads(self.native("search","--json","--np").stdout)),2)
        self.assertEqual(len(self.action("search",query="quartz")["hits"]),2)

    def test_37_flow_source_move_uses_flow_owner_and_keeps_history(self):
        self.project("alpha")
        self.write("Work/alpha/ProjectCentral/flows/.keep", b"")
        def flow(op, **values):
            values["project"] = "alpha"
            result = self.run_cmd([self.ctrl,"--json","--root",self.root,"action","run",f"projectcentral.flow.{op}",json.dumps(values)])
            return json.loads(result.stdout)["data"]
        record = flow("create",path="ProjectCentral/flows/original.md",actor="fixture",actor_kind="human")["flow"]
        plan = self.move(record["source_ref"], "ProjectCentral/flows/renamed.md")
        reading = flow("read",flow_ref=record["flow_ref"],actor="fixture",actor_kind="human")
        self.assertEqual(reading["flow"]["path"], "ProjectCentral/flows/renamed.md")
        self.assertEqual(reading["flow"]["source_ref"], record["source_ref"])
        self.assertEqual(reading["flow"]["revisions"], record["revisions"])
        self.action("move-rollback",plan_id=plan["plan_id"],quiesced=True)
        self.assertEqual(flow("read",flow_ref=record["flow_ref"],actor="fixture",actor_kind="human")["flow"]["path"], "ProjectCentral/flows/original.md")

    def test_38_record_adoption_retains_authored_description_title_and_tags(self):
        path = self.write("ordinary/note.txt")
        ref = self.register("ordinary/note.txt")
        database = self.base / "personal.db"
        self.native("create-db",str(database),db=database)
        self.native("add",path.as_uri(),"personal,second,third","--title","Human title","--description","My annotation","--no-web","--no-embed",db=database)
        self.action("adopt-db",database=str(database),quiesced=True,expected_revision=self.action("inspect")["revision"])
        row = json.loads(self.native("search","--json","--np").stdout)[0]
        native = self.action("inspect",record_id=row["id"])["native_record"]
        self.action("record-adopt",source_ref=ref,record_id=row["id"],record_revision=native["revision"],expected_revision=self.action("inspect")["revision"])
        self.action("refresh")
        current = json.loads(self.native("search","--json","--np").stdout)[0]
        self.assertEqual(current["title"],"Human title")
        self.assertIn("My annotation",current["description"])
        self.assertTrue({"personal","second","third"}.issubset(current["tags"]))

    def test_39_backup_includes_committed_wal_not_just_database_main_file(self):
        database = self.base / "wal.db"
        self.native("create-db",str(database),db=database)
        self.native("add","https://example.invalid/wal","--title","Before","--no-web","--no-embed",db=database)
        conn = sqlite3.connect(database)
        self.addCleanup(conn.close)
        conn.execute("pragma journal_mode=wal")
        conn.execute("pragma wal_autocheckpoint=0")
        # A separate harmless table avoids requiring bkmr's FTS/vector SQLite extensions.
        conn.execute("create table retained_wal_proof(value text)")
        conn.execute("insert into retained_wal_proof values ('committed WAL only')")
        conn.commit()
        self.assertTrue(Path(str(database)+"-wal").exists())
        receipt=self.action("adopt-db",database=str(database),quiesced=True)
        for candidate in [self.root/receipt["backup"], self.root/".central/bkmr/index.db"]:
            with sqlite3.connect(candidate) as adopted:
                self.assertEqual(adopted.execute("select value from retained_wal_proof").fetchone()[0],"committed WAL only")

    def test_40_skill_member_world_exclusion_is_not_bypassed_by_directory_binding(self):
        ref = self.skill()
        child = self.locate("Control/user/skills/astronomy/SKILL.md")["source"]["ref"]
        self.run_cmd([self.ctrl,"--json","--root",self.root,"action","run","central.world-relations.save",json.dumps({"scope":"root","record":{"schema":"central.world-relations/v1","ref":"control:base","revision":"r1","parent":None,"sources":[{"ref":child,"revision":"r1","authority":"human-authored","treatment":"retain-native"}]}})])
        self.run_cmd([self.ctrl,"--json","--root",self.root,"action","run","central.world-relations.save",json.dumps({"scope":"root","record":{"schema":"central.world-relations/v1","ref":"control:root","revision":"r1","parent":"control:base","excluded_sources":[child]}})])
        self.action("skill-tree",source_ref=ref,success=False)

    def test_41_available_but_unselected_skill_is_not_reported_as_projected(self):
        ref = self.promoted()
        self.ai("apply")
        self.assertEqual(self.action("projection-read",source_ref=ref)["records"],[])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl", required=True)
    parser.add_argument("--bkmr", required=True)
    parser.add_argument("--aikit", required=True)
    parser.add_argument("--case", help="Optional unittest substring")
    args = parser.parse_args()
    for field in ("ctrl","bkmr","aikit"):
        candidate = shutil.which(getattr(args,field)) or getattr(args,field)
        path = Path(candidate).resolve()
        if not path.is_file():
            parser.error(f"Missing {field} executable: {path}")
        setattr(Joined,field,str(path))
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Joined)
    if args.case:
        suite = unittest.TestSuite(t for t in suite if args.case in t.id())
        if not suite.countTestCases(): parser.error("No matching native test")
    outcome = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if outcome.wasSuccessful() else 1

if __name__ == "__main__":
    raise SystemExit(main())
