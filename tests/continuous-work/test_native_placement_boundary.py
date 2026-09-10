#!/usr/bin/env python3
"""Production Central CLI regressions, using disposable Worlds and actual Git.

CENTRAL_CAW_CTRL must identify an explicitly built native ctrl. No owner is mocked,
no policy is installed outside the disposable World, and no consumer recreates
placement semantics. This file intentionally fails on the pre-repair CAW binary.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class NativePlacementBoundary(unittest.TestCase):
    def setUp(self) -> None:
        binary = os.environ.get("CENTRAL_CAW_CTRL")
        if not binary:
            self.fail("CENTRAL_CAW_CTRL must name the exact native product binary")
        self.binary = Path(binary).resolve(strict=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="caw-native-boundary-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "isolated-home"
        self.home.mkdir()
        self.env = {
            "PATH": "/usr/bin:/bin", "HOME": str(self.home), "LANG": "C.UTF-8",
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "XDG_DATA_HOME": str(self.home / ".local/share"),
            "XDG_STATE_HOME": str(self.home / ".local/state"),
            "TZ": "UTC",
        }
        for relative in ["Control/user", "Control/relations", "Work/demo/src"]:
            (self.root / relative).mkdir(parents=True, exist_ok=True)
        self.policy_path = self.root / "Control/user/placement.json"
        self.policy = {
            "schema": "central.work-placement-policy/v1", "scope_ref": "control:root",
            "writable": [{"path": "Work/demo", "class": "repository"}],
            "protected": [], "enforcement": "native-actions",
            "required_coverage": ["file-content"], "lease_seconds": 300,
        }
        self.write_policy()
        self.token = "controlled-native-agent-credential-not-a-personal-secret"
        authority = {
            "schema": "central.native-action-authority/v1", "scope_ref": "control:root",
            "grants": [{
                "principal_ref": "agent:boundary-test", "actor_kind": "agent",
                "token_sha256": hashlib.sha256(self.token.encode()).hexdigest(),
                "scope_refs": ["control:root"],
                "actions": ["central.now.lifecycle"],
                "expires_at_unix_seconds": 18446744073709551615,
            }],
        }
        (self.root / "Control/user/authority.json").write_text(json.dumps(authority))
        relations = []
        for name, role in [("placement.json", "work-placement-policy"),
                           ("authority.json", "native-action-authority")]:
            path = f"Control/user/{name}"
            relations.append({
                "ref": f"central:source:control:root:{path}", "path": path,
                "roles": [role], "provenance": "human-adopted",
                "standing": "architecture-contract", "treatment": "projectcentral-user",
                "recognition": "controlled-fixture-not-personal-adoption",
                "recorded_at_unix_seconds": 1,
            })
        (self.root / "Control/relations/source-relations.json").write_text(json.dumps({
            "schema": "central.control.ground-relations/v1", "project_id": "control:root",
            "relations": relations,
        }))
        self.invocations: list[dict] = []

    def write_policy(self) -> None:
        self.policy_path.write_text(json.dumps(self.policy))

    def invoke(self, action: str, request: dict, *, authenticated: bool = False) -> dict:
        env = dict(self.env)
        if authenticated:
            env["CENTRAL_NATIVE_TOKEN"] = self.token
        result = subprocess.run(
            [str(self.binary), "--json", "--root", str(self.root), "action", "run",
             action, json.dumps(request)],
            cwd=self.root, env=env, stdin=subprocess.DEVNULL,
            capture_output=True, text=True, timeout=30, check=False,
        )
        try:
            response = json.loads(result.stdout)
        except ValueError as error:
            self.fail(f"{action}: invalid native result ({result.returncode}): "
                      f"{result.stdout!r} / {result.stderr!r}: {error}")
        self.invocations.append({"action": action, "exit": result.returncode,
                                 "request": request, "response": response})
        self.assertEqual(response["action"], action, response)
        self.assertEqual(result.returncode == 0, response["ok"], response)
        return response

    def success(self, action: str, request: dict, *, authenticated: bool = False) -> dict:
        response = self.invoke(action, request, authenticated=authenticated)
        self.assertTrue(response["ok"], response)
        return response["data"]

    def current_policy(self) -> dict:
        return self.success("central.work.policy", {})

    def allocate(self, task: str = "task:boundary") -> dict:
        return self.success("central.now.allocate", {
            "task_ref": task, "purpose": "Native placement boundary regression",
            "participant_refs": ["agent:boundary-test"], "source_refs": [],
            "expected_policy_revision": self.current_policy()["revision"],
        })

    def read_now(self, allocation: dict) -> dict:
        return self.success("central.now.read", {"now_ref": allocation["now_ref"]})

    def validate(self, allocation: dict, destination: str | Path, *, policy: dict | None = None) -> dict:
        reading = self.read_now(allocation)
        return self.invoke("central.work.validate", {
            "now_ref": allocation["now_ref"],
            "expected_now_revision": reading["revision"]["revision"],
            "expected_policy_revision": (policy or self.current_policy())["revision"],
            "destination": str(destination),
        })

    def lifecycle(self, allocation: dict, state: str, *, authenticated: bool = True) -> dict:
        reading = self.read_now(allocation)
        return self.invoke("central.now.lifecycle", {
            "now_ref": allocation["now_ref"], "expected_revision": reading["revision"]["revision"],
            "expected_policy_revision": self.current_policy()["revision"], "lifecycle": state,
        }, authenticated=authenticated)

    def test_explicit_protected_file_in_now_is_not_overridden(self) -> None:
        allocation = self.allocate()
        now = Path(allocation["writable_destination"])
        private = now / "retained" / "human.txt"
        private.parent.mkdir()
        private.write_bytes(b"retained human material\r\n")
        self.policy["protected"] = [str(private.relative_to(self.root))]
        self.write_policy()
        self.assertTrue(self.validate(allocation, now / "ordinary-plan.md")["ok"])
        self.assertFalse(self.validate(allocation, private)["ok"],
                         "Allocated NOW must not override an explicit protected source")
        self.assertEqual(private.read_bytes(), b"retained human material\r\n")

    def test_protected_descendant_does_not_allow_ambiguous_parent_mutation(self) -> None:
        allocation = self.allocate()
        private = Path(allocation["writable_destination"]) / "retained" / "human.txt"
        private.parent.mkdir()
        private.write_text("retained")
        self.policy["protected"] = [str(private.relative_to(self.root))]
        self.write_policy()
        self.assertFalse(self.validate(allocation, private.parent)["ok"],
                         "Untyped parent write approval could authorise deleting protected children")

    def test_explicit_whole_now_exclusion_blocks_allocation_before_publication(self) -> None:
        self.policy["protected"] = ["Control/agents/now"]
        self.write_policy()
        response = self.invoke("central.now.allocate", {
            "task_ref": "task:denied-allocation", "purpose": "No writes into protected aperture",
            "expected_policy_revision": self.current_policy()["revision"],
        })
        self.assertFalse(response["ok"], response)
        self.assertFalse((self.root / "Control/agents/now/clearings").exists())
        relations = json.loads((self.root / "Control/relations/source-relations.json").read_text())
        self.assertFalse(any("now-clearing" in r["roles"] for r in relations["relations"]))

    def test_quiescent_closed_and_archived_now_cannot_authorise_repository_writes(self) -> None:
        allocation = self.allocate()
        for state in ["quiescent", "closed", "archived"]:
            with self.subTest(state=state):
                self.assertTrue(self.lifecycle(allocation, state)["ok"])
                response = self.validate(allocation, "Work/demo/src/result.rs")
                self.assertFalse(response["ok"],
                                 f"{state} task must re-enter before authorising repository effects")

    def test_reentry_preserves_identity_artifacts_and_dirty_checkout(self) -> None:
        project = self.root / "Work/demo"
        def git(*args: str) -> bytes:
            return subprocess.check_output(["git", "-C", str(project), *args], env=self.env,
                                           stderr=subprocess.PIPE, timeout=30)
        git("init", "-q")
        git("config", "user.name", "Controlled CAW fixture")
        git("config", "user.email", "fixture@invalid.test")
        source = project / "src/lib.rs"
        source.write_text("// selected clean base\n")
        git("add", ".")
        git("commit", "-qm", "controlled selected base")
        source.write_text("// staged human change\n")
        git("add", "src/lib.rs")
        source.write_text("// unstaged human change\r\n")
        (project / "untracked.txt").write_bytes(b"untracked human bytes\x00")
        basis = (git("rev-parse", "HEAD"), git("diff", "--binary"),
                 git("diff", "--cached", "--binary"), source.read_bytes(),
                 (project / "untracked.txt").read_bytes(), (project / ".git/index").read_bytes())
        first, second = self.allocate("task:candidate-one"), self.allocate("task:candidate-two")
        self.assertNotEqual(first["now_ref"], second["now_ref"])
        artifact = Path(first["writable_destination"]) / "return.txt"
        artifact.write_bytes(b"unreceived candidate material")
        for state in ["closed", "archived", "active"]:
            self.assertTrue(self.lifecycle(first, state)["ok"])
        replay = self.allocate("task:candidate-one")
        self.assertEqual(replay["now_ref"], first["now_ref"])
        self.assertEqual(replay["writable_destination"], first["writable_destination"])
        self.assertFalse(replay["created"])
        self.assertEqual(artifact.read_bytes(), b"unreceived candidate material")
        self.assertTrue(self.validate(first, source)["ok"])
        self.assertEqual(basis, (git("rev-parse", "HEAD"), git("diff", "--binary"),
                         git("diff", "--cached", "--binary"), source.read_bytes(),
                         (project / "untracked.txt").read_bytes(), (project / ".git/index").read_bytes()))

    def test_stale_private_and_unauthenticated_sources_fail_without_effects(self) -> None:
        allocation = self.allocate()
        stale = self.current_policy()
        self.policy["protected"] = ["Work/demo/src/private.txt"]
        self.write_policy()
        self.assertFalse(self.validate(allocation, "Work/demo/src/ordinary.rs", policy=stale)["ok"])
        before = self.read_now(allocation)
        self.assertFalse(self.lifecycle(allocation, "closed", authenticated=False)["ok"])
        self.assertEqual(self.read_now(allocation)["revision"], before["revision"])
        (self.root / "Control/user/.no-agent-retrieval").write_text("withheld")
        self.assertFalse(self.invoke("central.work.policy", {})["ok"])

    def test_validation_requires_actual_now_producer(self) -> None:
        response = self.invoke("central.work.validate", {
            "now_ref": "central:now:absent", "expected_now_revision": "missing",
            "expected_policy_revision": self.current_policy()["revision"],
            "destination": "Work/demo/src/result.rs",
        })
        self.assertFalse(response["ok"], "Removing the native allocation must break validation")


if __name__ == "__main__":
    unittest.main(verbosity=2)
