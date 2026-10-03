#!/usr/bin/env python3
"""Real ctrl + bkmr + AIKit contract proof, restricted to disposable Worlds.

Run: python3 tests/bkmr_joined.py --ctrl /built/ctrl --bkmr /installed/bkmr --aikit /built/aikit
No mocks implement the owner operations. Missing binaries are errors, not skips.
"""
from __future__ import annotations
import argparse
import base64
import errno
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
        scratch = Path(__file__).resolve().parents[1] / "ProjectCentral/now/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="central-bkmr-joined-", dir=scratch)
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
        self.assertNotEqual(record["import_id"], record["id"], record)
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
        body = b"quartz native-owner disconnect must not deliver this retained body\n"
        source = self.write("Control/user/note.md", body)
        self.action("refresh")
        ref = self.locate("Control/user/note.md")["source"]["ref"]
        search = self.ai("knowledge", "search", "quartz")
        self.assertIn(ref, json.dumps(search))
        warm = self.ai("knowledge", "read", "source=" + ref)
        self.assertTrue(warm["ok"], warm)
        self.assertIn(body.decode().strip(), json.dumps(warm))
        basis = self.db_digest()
        inode = source.stat().st_ino
        missing_owner = self.base / "missing-ctrl"
        self.assertFalse(missing_owner.exists())
        disconnected = self.run_cmd(
            [self.aikit, "--json", "-C", self.root, "knowledge", "read", "source=" + ref],
            success=False, env=dict(self.env, CENTRAL_CTRL_BIN=str(missing_owner)))
        failure = json.loads(disconnected.stdout)
        self.assertEqual(failure["schema"], 1, failure)
        self.assertIs(failure["ok"], False, failure)
        # AIKit338 loses the unavailable-owner cause during attachment and
        # reports source_missing. This assertion characterises that cut; it
        # does not certify a truthful native unavailability classification.
        self.assertEqual(failure["error"]["code"], "knowledge.source_missing", failure)
        self.assertIn(ref, failure["error"]["message"])
        self.assertIsInstance(failure["error"]["details"], dict)
        self.assertNotIn("data", failure)
        self.assertNotIn("context", failure)
        self.assertNotIn(body.decode().strip(), disconnected.stdout)
        self.assertNotIn(body.decode().strip(), disconnected.stderr)
        self.assertEqual(source.read_bytes(), body)
        self.assertEqual(source.stat().st_ino, inode)
        self.assertEqual(self.db_digest(), basis)
        reopened = self.ai("knowledge", "read", "source=" + ref)
        self.assertTrue(reopened["ok"], reopened)
        self.assertIn(body.decode().strip(), json.dumps(reopened))
        self.assertEqual(source.read_bytes(), body)
        self.assertEqual(source.stat().st_ino, inode)
        self.assertEqual(self.db_digest(), basis)

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

    def test_37_retired_flow_registry_stays_retained_through_current_source_moves(self):
        self.project("alpha")
        legacy = self.write("Work/alpha/.central/flows.json", b"retained retired registry bytes\n")
        history = self.write("Work/alpha/.central/flow-revisions/retained.bin", bytes([0, 255, 9]))
        source = self.write("Work/alpha/ProjectCentral/user/flows/original.md", "current ordinary Flow source\n")
        ref = self.action("locate", "alpha", path=str(source), binding_only=True)["source"]["ref"]
        before = self.material_basis(legacy), self.material_basis(history)
        source_before = self.material_basis(source)
        unavailable = self.run_cmd([
            self.ctrl, "--json", "--root", self.root, "action", "run", "projectcentral.flow.create",
            json.dumps({"project":"alpha", "path":"ProjectCentral/flows/new.md", "actor":"fixture", "actor_kind":"human"})
        ], success=False)
        self.assertEqual(json.loads(unavailable.stdout)["error"]["code"], "invalid_input")
        self.assertEqual(json.loads(unavailable.stdout)["error"]["message"], "Unknown Action: projectcentral.flow.create")
        self.assertFalse((self.root / "Work/alpha/ProjectCentral/flows/new.md").exists())
        self.action("move-plan", source_ref=ref, destination=".central/flows.json", success=False)
        plan = self.move(ref, "ProjectCentral/user/flows/renamed.md")
        moved = self.root / "Work/alpha/ProjectCentral/user/flows/renamed.md"
        self.assertFalse(source.exists())
        self.assertEqual(self.material_basis(moved), source_before)
        self.assertEqual(self.action("resolve", "alpha", source_ref=ref, content=True)["content"], source_before[-1].decode())
        self.assertEqual(self.action("locate", "alpha", path=str(moved), binding_only=True)["source"]["ref"], ref)
        self.assertEqual((self.material_basis(legacy), self.material_basis(history)), before)
        self.action("move-rollback", plan_id=plan["plan_id"], quiesced=True)
        self.assertTrue(source.is_file())
        self.assertFalse(moved.exists())
        self.assertEqual(self.material_basis(source), source_before)
        self.assertEqual(self.action("resolve", "alpha", source_ref=ref, content=True)["content"], source_before[-1].decode())
        self.assertEqual((self.material_basis(legacy), self.material_basis(history)), before)

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


    def declared(self, rows, project=None, **extensions):
        root = self.root if project is None else self.root / "Work" / project
        path = root / ("Control/relations/source-relations.json" if project is None
                       else "ProjectCentral/relations/source-relations.json")
        path.parent.mkdir(parents=True, exist_ok=True)
        doc = {"schema": "central.control.ground-relations/v1" if project is None
               else "central.project.ground-relations/v1",
               "project_id": "control:root" if project is None else project,
               "relations": rows, **extensions}
        path.write_bytes(json.dumps(doc).encode())
        return path

    @staticmethod
    def relation(reference, path, **extensions):
        return {"ref": reference, "path": path, "roles": ["agent-governance-source"],
                "provenance": "unresolved", "standing": "unspecified",
                "treatment": "retain-native-in-place", **extensions}

    @staticmethod
    def material_basis(path):
        metadata = path.stat()
        return (metadata.st_dev, metadata.st_ino, metadata.st_size,
                metadata.st_mtime_ns, metadata.st_mode, path.read_bytes())

    def tree_basis(self):
        return {str(path.relative_to(self.root)): self.material_basis(path)
                for path in self.root.rglob("*") if path.is_file() and not path.is_symlink()}

    def test_42_binding_only_keeps_opaque_owner_without_reading_unreadable_body(self):
        self.assertNotEqual(os.geteuid(), 0, "Actual unreadable-body proof requires a nonroot execution owner")
        path = self.write("Control/user/body.md", "retained unreadable actual body\n")
        ref = "opaque:declared:actual-body"
        declared = self.declared([self.relation(ref, "Control/user/body.md", retained={"inner": 7})],
                                 extension={"retained": [3, 1]})
        before = self.material_basis(path), self.material_basis(declared)
        mode = path.stat().st_mode & 0o7777
        try:
            path.chmod(0)
            with self.assertRaises(OSError) as observed:
                path.read_bytes()
            self.assertEqual(observed.exception.errno, errno.EACCES)
            for op, request in [("resolve", {"source_ref": ref}), ("locate", {"path": str(path)})]:
                result = self.action(op, binding_only=True, **request)
                self.assertEqual(result["ownership"], "owned")
                self.assertEqual(result["source"]["ref"], ref)
                self.assertEqual(result["world_ref"], "control:root")
                self.assertEqual(result["relation_revision"], fnv(declared.read_bytes()))
                self.assertEqual(result["material_metadata_basis"]["inode"], path.stat().st_ino)
                for name in ["revision", "content", "content_encoding", "projection", "skill_manifest"]:
                    self.assertNotIn(name, result)
            failure = self.action("resolve", source_ref=ref, content=False, success=False)
            self.assertEqual(failure["error"]["details"]["ownership"], "known")
            self.assertEqual(failure["error"]["details"]["io_error"]["raw_os_error"], observed.exception.errno)
            self.assertEqual(failure["error"]["details"]["effects"], "none")
        finally:
            path.chmod(mode)
        self.assertEqual((self.material_basis(path), self.material_basis(declared)), before)
        self.assertFalse((self.root / ".central").exists())

    def test_43_healthy_unregistered_is_not_minted_and_invalid_metadata_never_becomes_absence(self):
        plain = self.write("ordinary.txt", "not accepted as a native Source\n")
        unknown = "central:source:control:root:ordinary.txt"
        result = self.action("resolve", source_ref=unknown, binding_only=True)
        self.assertEqual(result, {"ownership": "unregistered", "binding_only": True, "source_ref": unknown})
        result = self.action("locate", path=str(plain), binding_only=True)
        self.assertEqual(result, {"ownership": "unregistered", "binding_only": True, "requested_path": str(plain)})
        rows = [self.relation("opaque:first", "Control/user/note.md"),
                self.relation("opaque:second", "Control/user//note.md")]
        declaration = self.declared(rows)
        before = declaration.read_bytes()
        result = self.action("resolve", source_ref="opaque:absent", binding_only=True, success=False)
        self.assertNotEqual(result["error"]["details"]["ownership"], "unregistered")
        self.assertEqual(declaration.read_bytes(), before)
        for invalid_document in [b"{malformed actual declaration", b"null"]:
            declaration.write_bytes(invalid_document)
            result = self.action("resolve", source_ref="opaque:absent", binding_only=True, success=False)
            self.assertEqual(result["error"]["details"]["effects"], "none")

    def test_44_known_absence_withdrawal_and_sibling_reopen_preserve_the_owner(self):
        private = self.write("Control/user/private/body.md", "unselected-private-body\n")
        sibling = self.write("Control/user/open.md", "selected sibling\n")
        self.declared([self.relation("opaque:private", "Control/user/private/body.md"),
                       self.relation("opaque:missing", "Control/user/missing.md"),
                       self.relation("opaque:sibling", "Control/user/open.md")])
        missing = self.action("resolve", source_ref="opaque:missing", binding_only=True, success=False)
        self.assertEqual(missing["error"]["code"], "central.file_map_not_found")
        self.assertEqual(missing["error"]["details"]["ownership"], "known")
        before = self.material_basis(private), self.material_basis(sibling)
        marker = private.parent / ".no-agent-retrieval"
        marker.write_bytes(b"")
        for op, request in [("resolve", {"source_ref": "opaque:private"}), ("locate", {"path": str(private)})]:
            denied = self.action(op, binding_only=True, success=False, **request)
            self.assertEqual(denied["error"]["code"], "central.file_map_denied")
            self.assertEqual(denied["error"]["details"]["ownership"], "known")
            self.assertEqual(denied["error"]["details"]["material_state"], "withheld")
            self.assertIsNone(denied["error"]["details"]["io_error"])
            self.assertNotIn("private/body.md", json.dumps(denied))
            self.assertNotIn("unselected-private-body", json.dumps(denied))
        self.assertEqual(self.action("resolve", source_ref="opaque:sibling", binding_only=True)["source"]["ref"], "opaque:sibling")
        marker.unlink()
        self.assertEqual(self.action("resolve", source_ref="opaque:private", binding_only=True)["source"]["ref"], "opaque:private")
        self.assertEqual((self.material_basis(private), self.material_basis(sibling)), before)

    def test_45_target_admission_precedes_payload_and_skill_projection_parsing(self):
        self.project("alpha")
        source = self.write("Control/user/skills/fixture/SKILL.md", "actual native skill body\n")
        self.write("Control/user/skills/fixture/skill.json", "malformed actual projection metadata")
        self.declared([self.relation("opaque:skill", "Control/user/skills/fixture/SKILL.md")])
        # Existing accepted Source identity is useful without demanding a separate Skill projection.
        self.assertEqual(self.action("resolve", source_ref="opaque:skill", binding_only=True)["source"]["ref"], "opaque:skill")
        self.action("resolve", source_ref="opaque:skill", content=False, success=False)
        denied = self.action("resolve", "alpha", source_ref="opaque:skill", content=False, success=False)
        self.assertEqual(denied["error"]["details"]["failure_stage"], "project_admission")
        self.assertEqual(denied["error"]["details"]["material_state"], "withheld")
        marker = source.parent / ".no-agent-retrieval"
        marker.write_bytes(b"")
        denied = self.action("resolve", source_ref="opaque:skill", content=False, success=False)
        self.assertEqual(denied["error"]["code"], "central.file_map_denied")
        self.assertNotIn("skill.json", json.dumps(denied))
        source.unlink()
        os.mkfifo(source)
        denied = self.action("resolve", source_ref="opaque:skill", content=False, success=False)
        self.assertEqual(denied["error"]["code"], "central.file_map_denied")
        source.unlink()
        source.write_bytes(b"actual native skill body\n")

    def test_46_enclosing_world_withdrawal_and_world_exclusion_precede_project_body_read(self):
        project = self.project("alpha")
        source = self.write("Work/alpha/ProjectCentral/user/body.md", "retained project body\n")
        ref = "opaque:project:body"
        self.declared([self.relation(ref, "ProjectCentral/user/body.md")], "alpha")
        before = self.material_basis(source)
        marker = self.root / "Work/.no-agent-retrieval"
        marker.write_bytes(b"")
        denied = self.action("resolve", "alpha", source_ref=ref, binding_only=True, success=False)
        self.assertEqual(denied["error"]["details"]["failure_stage"], "world_source_admission")
        marker.unlink()
        self.assertEqual(self.action("resolve", "alpha", source_ref=ref, binding_only=True)["world_ref"], "project:alpha")
        for scope, owner, record in [
            ("root", None, {"schema":"central.world-relations/v1","ref":"control:root","revision":"r1","parent":None,"sources":[{"ref":ref,"revision":"r1","authority":"human-authored","treatment":"retain-native"}]}),
            ("project", "alpha", {"schema":"central.world-relations/v1","ref":"alpha","revision":"r1","parent":"control:root","excluded_sources":[ref]})]:
            self.run_cmd([self.ctrl, "--json", "--root", self.root, "action", "run", "central.world-relations.save",
                          json.dumps({"scope":scope,"project":owner,"record":record})])
        denied = self.action("resolve", "alpha", source_ref=ref, content=False, success=False)
        self.assertEqual(denied["error"]["details"]["failure_stage"], "context_admission")
        self.assertEqual(self.material_basis(source), before)
        self.assertTrue(project.is_dir())

    def test_47_declaration_capacity_is_eight_mebibytes_without_widening_source_delivery(self):
        source = self.write("Control/user/body.md", "source body\n")
        ref = "opaque:metadata-capacity"
        declaration = self.declared([self.relation(ref, "Control/user/body.md")],
                                    retained_padding="x" * (4 * 1024 * 1024 + 1))
        self.assertLess(declaration.stat().st_size, 8 * 1024 * 1024)
        before = self.material_basis(declaration), self.material_basis(source)
        result = self.action("resolve", source_ref=ref, binding_only=True)
        self.assertEqual(result["relation_revision"], fnv(declaration.read_bytes()))
        self.assertNotIn("retained_padding", result)
        self.assertEqual((self.material_basis(declaration), self.material_basis(source)), before)
        self.declared([self.relation(ref, "Control/user/body.md")], retained_padding="x" * (8 * 1024 * 1024))
        result = self.action("resolve", source_ref=ref, binding_only=True, success=False)
        self.assertEqual(result["error"]["details"]["effects"], "none")
        self.assertEqual(result["error"]["details"]["io_error"]["kind"], "InvalidData")
        declaration = self.declared([self.relation(ref, "Control/user/body.md")])
        source.write_bytes(b"x" * (4 * 1024 * 1024 + 1))
        self.action("resolve", source_ref=ref, content=True, success=False)
        self.assertEqual(source.stat().st_size, 4 * 1024 * 1024 + 1)

    def test_48_actual_cli_binary_and_action_share_current_index_privacy_without_writes(self):
        source = self.write("Control/agents/governance/open.md", "# Open native title\nselected-open-body\n")
        private = self.write("Control/agents/governance/private/hidden.md", "# Hidden native title\nunselected-private-body\n")
        self.write("Control/agents/governance/private/.no-agent-retrieval", b"")
        before = self.tree_basis()
        common = [self.ctrl, "--json", "--root", self.root]
        cli = self.run_cmd([*common, "control", "index"])
        owner = self.run_cmd([*common, "action", "run", "control.index", "{}"])
        self.assertEqual(json.loads(cli.stdout), json.loads(owner.stdout))
        for text in ["hidden.md", "Hidden native title", "unselected-private-body"]:
            self.assertNotIn(text, cli.stdout)
        self.assertIn("Open native title", cli.stdout)
        self.assertEqual(self.tree_basis(), before)
        self.assertEqual(source.read_bytes(), b"# Open native title\nselected-open-body\n")
        self.assertEqual(private.read_bytes(), b"# Hidden native title\nunselected-private-body\n")

    def test_49_binding_mode_optional_provider_standalone_alias_links_and_adopted_sources(self):
        project = self.project("alpha")
        source = self.write("Work/alpha/docs/wiki.json", '{"profile":"okf-wiki/v1","objects":[]}')
        manifest = project / "ProjectCentral/project.json"
        document = json.loads(manifest.read_bytes())
        document["wiki"]["adopted_sources"] = ["docs/wiki.json"]
        manifest.write_bytes(json.dumps(document).encode())
        ref = "opaque:retained:wiki"
        self.declared([self.relation(ref, "docs/wiki.json")], "alpha")
        env = dict(self.env, CENTRAL_BKMR_BIN=str(self.base / "absent-bkmr"))
        result = self.action("resolve", "alpha", source_ref=ref, binding_only=True, env=env)
        self.assertEqual(result["source"]["ref"], ref)
        self.assertEqual(result["world_ref"], "project:alpha")
        self.link(ref, "Control/user/project-wiki.json")
        result = self.action("locate", path="Control/user/project-wiki.json", binding_only=True, env=env)
        self.assertEqual(result["source"]["ref"], ref)
        self.assertEqual(result["encountered_link"]["world_ref"], "control:root")
        alias = self.base / "project-alias"
        alias.symlink_to(project, target_is_directory=True)
        result = self.run_cmd([self.ctrl, "--json", "--root", alias, "action", "run", "central.file-map.resolve",
                               json.dumps({"source_ref":ref,"binding_only":True})], env=env)
        parsed = json.loads(result.stdout)["data"]["result"]
        self.assertEqual(parsed["world_ref"], "project:alpha")
        self.assertEqual(parsed["source"]["ref"], ref)
        self.assertEqual(parsed["project"], None)
        self.assertFalse((project / ".central/bkmr").exists())
        self.assertEqual(source.read_bytes(), b'{"profile":"okf-wiki/v1","objects":[]}')

    def test_50_binding_mode_validation_and_owner_metadata_eacces_are_not_no_owner(self):
        self.assertNotEqual(os.geteuid(), 0, "Actual owner IO proof requires a nonroot execution owner")
        body = self.write("Control/user/body.md", "actual body\n")
        ref = "opaque:owner-io"
        declaration = self.declared([self.relation(ref, "Control/user/body.md")])
        for values in [{"binding_only": "true"}, {"binding_only": True, "content": True},
                       {"binding_only": True, "expected_revision": "absent"}]:
            self.action("resolve", source_ref=ref, success=False, **values)
        mode = declaration.stat().st_mode & 0o7777
        before = self.material_basis(declaration), self.material_basis(body)
        try:
            declaration.chmod(0)
            with self.assertRaises(OSError) as observed:
                declaration.read_bytes()
            failed = self.action("resolve", source_ref=ref, binding_only=True, success=False)
            self.assertNotEqual(failed["error"]["details"]["ownership"], "unregistered")
            self.assertEqual(failed["error"]["details"]["io_error"]["raw_os_error"], observed.exception.errno)
        finally:
            declaration.chmod(mode)
        self.assertEqual((self.material_basis(declaration), self.material_basis(body)), before)



    def test_51_registered_external_owner_unavailability_never_becomes_standalone_or_no_owner(self):
        self.assertNotEqual(os.geteuid(), 0, "Actual Project declaration IO requires a nonroot owner")
        project = self.project("external")
        body = self.write("Work/external/ProjectCentral/user/body.md", "retained external body\n")
        self.declared([self.relation("opaque:external", "ProjectCentral/user/body.md")], "external")
        outside = self.base / "NativeProject"
        project.rename(outside)
        body = outside / "ProjectCentral/user/body.md"
        self.action("scope-register", name="external", path=str(outside), allow_external=True,
                    expected_revision=self.action("inspect")["revision"])
        self.assertEqual(self.action("resolve", "external", source_ref="opaque:external", binding_only=True)["world_ref"], "project:external")
        manifest = outside / "ProjectCentral/project.json"
        before = self.material_basis(manifest), self.material_basis(body)
        mode = manifest.stat().st_mode & 0o7777
        try:
            manifest.chmod(0)
            with self.assertRaises(OSError) as observed:
                manifest.read_bytes()
            failed = self.action("resolve", "external", source_ref="opaque:external", binding_only=True, success=False)
            self.assertEqual(failed["error"]["details"]["ownership"], "known")
            self.assertEqual(failed["error"]["details"]["io_error"]["raw_os_error"], observed.exception.errno)
        finally:
            manifest.chmod(mode)
        self.assertEqual((self.material_basis(manifest), self.material_basis(body)), before)
        absent = self.base / "RetainedProject"
        outside.rename(absent)
        failed = self.action("resolve", "external", source_ref="opaque:external", binding_only=True, success=False)
        self.assertEqual(failed["error"]["details"]["ownership"], "known")
        self.assertEqual(failed["error"]["details"]["failure_stage"], "owner_root")
        absent.rename(outside)
        self.assertEqual(self.action("resolve", "external", source_ref="opaque:external", binding_only=True)["source"]["ref"], "opaque:external")
        self.assertEqual(self.material_basis(body), before[1])



    def test_52_registered_native_files_and_skills_keep_existing_owner_semantics(self):
        ordinary = self.write("Control/agents/governance/native-note.md", "native source metadata\n")
        self.skill()
        native = {entry["path"]: entry["source"] for entry in self.action("inspect")["resources"]}
        selected = [ordinary, self.root / "Control/user/skills/astronomy/SKILL.md"]
        before = [self.material_basis(path) for path in selected]
        for path in selected:
            binding = native[str(path)]
            ref = self.register(path.relative_to(self.root))
            self.assertEqual(ref, binding["ref"])
            for op, request in [("resolve", {"source_ref":ref}), ("locate", {"path":str(path)})]:
                reading = self.action(op, binding_only=True, **request)
                self.assertEqual(reading["source"], binding)
                self.assertNotIn("revision", reading)
                self.assertNotIn("content", reading)
            self.assertEqual(self.action("resolve", source_ref=ref)["source"], binding)
        self.assertEqual([self.material_basis(path) for path in selected], before)
        self.assertEqual(native[str(selected[1])]["provenance"], "human-authored")
        self.assertEqual(native[str(selected[1])]["standing"], "active")
        self.assertEqual(native[str(selected[1])]["roles"], ["skill-source"])

    def test_53_foreign_explicit_ref_cannot_hide_a_native_fallback_owner(self):
        self.project("alpha")
        foreign = self.write("Work/alpha/ProjectCentral/user/native.md", "selected foreign owner\n")
        root_body = self.write("Control/user/native.md", "selected root owner\n")
        ref = self.action("locate", "alpha", path=str(foreign), binding_only=True)["source"]["ref"]
        declaration = self.declared([self.relation(ref, "Control/user/native.md")])
        before = [self.material_basis(path) for path in [foreign, root_body, declaration]]
        failure = self.action("resolve", source_ref=ref, binding_only=True, success=False)
        self.assertEqual(failure["error"]["code"], "central.file_map_conflict")
        self.assertEqual(failure["error"]["details"]["ownership"], "known")
        self.assertNotIn("selected foreign owner", json.dumps(failure))
        self.assertNotIn("selected root owner", json.dumps(failure))
        self.assertEqual([self.material_basis(path) for path in [foreign, root_body, declaration]], before)

    def test_54_prior_resource_location_cannot_hide_a_new_native_project_owner(self):
        source = self.write("Work/alpha/ProjectCentral/user/native.md", "retained same physical source\n")
        prior = self.register(source, allow_external=True)
        # Genuine later authored Project declaration; the retained root resource
        # is historical native state, not an injected fake owner result.
        manifest = self.write("Work/alpha/ProjectCentral/project.json", json.dumps({
            "schema":"central.project/v1", "project_id":"alpha", "human_source":"ProjectCentral/user",
            "wiki":{"profile":"okf-wiki/v1", "source":"ProjectCentral/agents/wiki/wiki.json"}}))
        before = self.material_basis(source), self.material_basis(manifest)
        failure = self.action("locate", path=str(source), binding_only=True, success=False)
        self.assertEqual(failure["error"]["code"], "central.file_map_conflict")
        self.assertEqual(failure["error"]["details"]["ownership"], "known")
        retained = self.action("inspect")["resources"]
        self.assertTrue(any(item["source"]["ref"] == prior and item["path"] == str(source) for item in retained))
        self.assertEqual((self.material_basis(source), self.material_basis(manifest)), before)

    def native_inherited_exclusion(self, source_ref):
        project = self.project("alpha")
        self.link(source_ref, "ProjectCentral/user/root-note.md", "alpha")
        prior = self.action("resolve", "alpha", source_ref=source_ref, binding_only=True)
        self.assertEqual(prior["source"]["ref"], source_ref)
        self.assertEqual(prior["world_ref"], "control:root")
        paths = []
        for scope, name, owner, record in [
            ("root", None, self.root, {"schema":"central.world-relations/v1", "ref":"control:root", "revision":"r1", "parent":None,
                "sources":[{"ref":source_ref, "revision":"r1", "authority":"human-authored", "treatment":"retain-native"}],
                "retained_extension":{"actual":True}}),
            ("project", "alpha", project, {"schema":"central.world-relations/v1", "ref":"alpha", "revision":"r1", "parent":"control:root",
                "excluded_sources":[source_ref]})
        ]:
            output = self.run_cmd([self.ctrl, "--json", "--root", self.root, "action", "run", "central.world-relations.save",
                                  json.dumps({"scope":scope, "project":name, "record":record})])
            receipt = json.loads(output.stdout)
            self.assertTrue(receipt["ok"], receipt)
            paths.append(owner / receipt["data"]["source_path"])
        self.assert_native_inherited_exclusion(source_ref)
        return paths

    def assert_native_inherited_exclusion(self, source_ref):
        result = self.run_cmd([self.ctrl, "--json", "--root", self.root, "action", "run", "central.world.effective-sources",
                              json.dumps({"scope":"project", "project":"alpha", "world_ref":"alpha"})])
        sources = json.loads(result.stdout)["data"]["sources"]
        selected = [row for row in sources if row["ref"] == source_ref]
        self.assertEqual(len(selected), 1, sources)
        self.assertEqual(selected[0]["state"], "excluded", selected)
        self.assertEqual(selected[0]["propagation_path"], ["control:root", "alpha"], selected)

    def test_55_native_world_store_broken_links_fail_current_context_and_reopen_without_writes(self):
        source = self.write("Control/user/excluded.md", "actual native excluded body\n")
        ref = self.action("locate", path=str(source), binding_only=True)["source"]["ref"]
        record_paths = self.native_inherited_exclusion(ref)
        container = record_paths[0].parent
        before = [self.material_basis(path) for path in [source, *record_paths]]
        self.action("resolve", "alpha", source_ref=ref, binding_only=True, success=False)
        retained = self.base / "retained-worlds"
        container.rename(retained)
        container.symlink_to(self.base / "actual-absent-container", target_is_directory=True)
        link_inode = container.lstat().st_ino
        try:
            for metadata_only in [True, False]:
                failure = self.action("resolve", "alpha", source_ref=ref, binding_only=metadata_only, success=False)
                self.assertEqual(failure["error"]["details"]["material_state"], "unavailable")
                self.assertEqual(failure["error"]["details"]["effects"], "none")
                self.assertNotIn("actual native excluded body", json.dumps(failure))
            self.assertEqual(container.lstat().st_ino, link_inode)
        finally:
            container.unlink()
            retained.rename(container)
        self.assert_native_inherited_exclusion(ref)
        failure = self.action("resolve", "alpha", source_ref=ref, binding_only=True, success=False)
        self.assertEqual(failure["error"]["details"]["material_state"], "withheld")
        self.assertEqual([self.material_basis(path) for path in [source, *record_paths]], before)

    def test_56_native_world_store_eacces_preserves_actual_cause_and_exclusion(self):
        self.assertNotEqual(os.geteuid(), 0, "Actual native World store EACCES proof requires a nonroot execution owner")
        source = self.write("Control/user/excluded.md", "retained excluded body\n")
        ref = self.action("locate", path=str(source), binding_only=True)["source"]["ref"]
        record_paths = self.native_inherited_exclusion(ref)
        container = record_paths[0].parent
        before = [self.material_basis(path) for path in [source, *record_paths]]
        mode = container.stat().st_mode & 0o7777
        try:
            container.chmod(0)
            with self.assertRaises(OSError) as actual:
                list(container.iterdir())
            self.assertEqual(actual.exception.errno, errno.EACCES)
            failure = self.action("resolve", "alpha", source_ref=ref, binding_only=True, success=False)
            self.assertEqual(failure["error"]["details"]["material_state"], "unavailable")
            self.assertEqual(failure["error"]["details"]["io_error"]["kind"], "PermissionDenied")
            self.assertEqual(failure["error"]["details"]["io_error"]["raw_os_error"], actual.exception.errno)
            self.assertNotIn("retained excluded body", json.dumps(failure))
        finally:
            container.chmod(mode)
        self.assert_native_inherited_exclusion(ref)
        failure = self.action("resolve", "alpha", source_ref=ref, binding_only=True, success=False)
        self.assertEqual(failure["error"]["details"]["material_state"], "withheld")
        self.assertEqual([self.material_basis(path) for path in [source, *record_paths]], before)


    def test_57_selected_project_skill_source_keeps_literal_accepted_identity_without_bulk_fallback(self):
        project = self.project("alpha")
        body = self.write("Work/alpha/ProjectCentral/user/skills/selected/SKILL.md", "actual selected Project body\n")
        metadata = self.write("Work/alpha/ProjectCentral/user/skills/selected/skill.json", "actual malformed unoverridden metadata")
        ref = "opaque:accepted-project-skill"
        declaration = self.declared([self.relation(ref, "ProjectCentral/user/skills/selected/SKILL.md")], "alpha")
        before = [self.material_basis(path) for path in [body, metadata, declaration]]
        result = self.action("resolve", "alpha", source_ref=ref, binding_only=True)
        self.assertEqual(result["source"]["ref"], ref)
        self.assertEqual(result["world_ref"], "project:alpha")
        self.assertNotIn("revision", result)
        self.assertNotIn("content", result)
        self.action("resolve", "alpha", source_ref=ref, content=False, success=False)
        self.action("inspect", "alpha", success=False)
        self.assertEqual([self.material_basis(path) for path in [body, metadata, declaration]], before)
        self.assertTrue(project.is_dir())

    def test_58_selected_native_skill_metadata_eacces_is_unavailable_and_reopens_exact_source(self):
        self.assertNotEqual(os.geteuid(), 0, "Actual selected Skill metadata EACCES requires a nonroot owner")
        self.skill()
        body = self.root / "Control/user/skills/astronomy/SKILL.md"
        metadata = body.parent / "skill.json"
        ref = self.action("locate", path=str(body), binding_only=True)["source"]["ref"]
        before = self.material_basis(body), self.material_basis(metadata)
        mode = metadata.stat().st_mode & 0o7777
        try:
            metadata.chmod(0)
            with self.assertRaises(OSError) as actual:
                metadata.read_bytes()
            failure = self.action("resolve", source_ref=ref, binding_only=True, success=False)
            self.assertEqual(failure["error"]["details"]["ownership"], "known")
            self.assertEqual(failure["error"]["details"]["material_state"], "unavailable")
            self.assertEqual(failure["error"]["details"]["io_error"]["raw_os_error"], actual.exception.errno)
        finally:
            metadata.chmod(mode)
        reading = self.action("resolve", source_ref=ref, binding_only=True)
        self.assertEqual(reading["source"]["provenance"], "human-authored")
        self.assertEqual(reading["source"]["standing"], "active")
        self.assertEqual((self.material_basis(body), self.material_basis(metadata)), before)



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
