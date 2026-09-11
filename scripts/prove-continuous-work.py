#!/usr/bin/env python3
"""Exercise real Central policy/NOW Actions in a disposable World, never private Control."""
import argparse
import concurrent.futures
import json
import pathlib
import subprocess
import tempfile


def run(binary: str) -> dict:
    with tempfile.TemporaryDirectory(prefix="central-consumer-proof-") as directory:
        root = pathlib.Path(directory)
        (root / "Control/user").mkdir(parents=True)
        (root / "Control/relations").mkdir()
        (root / "Work/demo/src").mkdir(parents=True)
        source = "Control/user/placement-policy.json"
        reference = "central:source:control:root:" + source
        policy = {
            "schema": "central.work-placement-policy/v1",
            "scope_ref": "control:root",
            "writable": [{"path": "Work/demo", "class": "repository"}],
            "protected": [],
            "enforcement": "harness-interception",
            "required_coverage": ["file-content", "file-creation", "file-removal", "rename-link", "truncate", "descendant-processes"],
            "lease_seconds": 300,
        }
        (root / source).write_text(json.dumps(policy), encoding="utf-8")
        relations = {
            "schema": "central.control.ground-relations/v1",
            "project_id": "control:root",
            "relations": [{
                "ref": reference, "path": source, "roles": ["work-placement-policy"],
                "provenance": "human-adopted", "standing": "architecture-contract",
                "treatment": "projectcentral-user",
                "recognition": "controlled-proof-fixture-not-personal-adoption",
                "recorded_at_unix_seconds": 1,
            }],
        }
        (root / "Control/relations/source-relations.json").write_text(json.dumps(relations), encoding="utf-8")

        def invoke(action: str, request: dict) -> dict:
            completed = subprocess.run(
                [binary, "--json", "--root", str(root), "action", "run", action, json.dumps(request)],
                text=True, capture_output=True, check=False, timeout=30,
            )
            try:
                return json.loads(completed.stdout)
            except json.JSONDecodeError as error:
                raise RuntimeError(f"{action}: {completed.returncode}: {completed.stdout} / {completed.stderr}") from error

        policy_result = invoke("central.work.policy", {})
        assert policy_result["ok"], policy_result
        request = {
            "task_ref": "task:consumer-proof", "purpose": "Native consumer request/result proof",
            "expected_policy_revision": policy_result["data"]["revision"],
        }
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            allocations = list(pool.map(lambda _: invoke("central.now.allocate", request), range(8)))
        assert all(result["ok"] for result in allocations), allocations
        allocation = next(result for result in allocations if result["data"]["created"])
        assert sum(result["data"]["created"] for result in allocations) == 1
        assert len({result["data"]["now_ref"] for result in allocations}) == 1
        assert len({result["data"]["revision"]["revision"] for result in allocations}) == 1
        assert pathlib.Path(allocation["data"]["writable_destination"]).is_dir()
        checks = []
        for destination, allowed in [
            ("Work/demo/src/main.rs", True), ("Work/scratch.diff", False),
            (allocation["data"]["writable_destination"] + "/plan.md", True),
        ]:
            validation_request = {
                "now_ref": allocation["data"]["now_ref"],
                "expected_now_revision": allocation["data"]["revision"]["revision"],
                "expected_policy_revision": policy_result["data"]["revision"],
                "destination": destination,
            }
            response = invoke("central.work.validate", validation_request)
            assert response["ok"] == allowed, response
            checks.append({"request": validation_request, "result": response})
        return {
            "schema": "central.consumer-native-proof/v1",
            "world": "temporary and deleted after this run", "native_binary": binary,
            "same_task_processes": 8, "created_sources": 1,
            "policy_result": policy_result, "allocation_request": request,
            "allocation_result": allocation, "placement_checks": checks,
            "personal_installation": False,
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl", required=True)
    arguments = parser.parse_args()
    print(json.dumps(run(arguments.ctrl), indent=2))
