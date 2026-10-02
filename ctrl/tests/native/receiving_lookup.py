#!/usr/bin/env python3
"""Real receiving lost-ack proof; only an explicit qualified ctrl and owned native scratch.
No receipt/hash reconstruction, mocks, automatic resend or personal installation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

CAPTURE = 1024 * 1024
TOKEN = "receiving-lookup-controlled-agent-not-a-real-secret"


def run(binary: str) -> dict:
    binary = str(Path(binary).resolve(strict=True))
    scratch = Path(__file__).resolve().parents[3] / "ProjectCentral/now/tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="receiving-lost-ack-", dir=scratch) as owned:
        root = Path(owned)
        (root / "Control/user").mkdir(parents=True)
        (root / "Control/relations").mkdir()
        (root / "Work/demo").mkdir(parents=True)
        policy = {"schema":"central.work-placement-policy/v1","scope_ref":"control:root",
            "writable":[{"path":"Work/demo","class":"repository"}],
            "enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}
        authority = {"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[{
            "principal_ref":"agent:receiving-lost-ack-fixture","actor_kind":"agent",
            "token_sha256":hashlib.sha256(TOKEN.encode()).hexdigest(),"scope_refs":["control:root"],
            "actions":["central.receiving.submit"],"expires_at_unix_seconds":9999999999}]}
        relations = []
        for name, role, value in [("placement.json","work-placement-policy",policy),
                                   ("authority.json","native-action-authority",authority)]:
            member = "Control/user/" + name
            (root / member).write_text(json.dumps(value),encoding="utf-8")
            relations.append({"ref":"central:source:control:root:"+member,"path":member,"roles":[role],
                "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
                "recognition":"controlled-proof-fixture-not-personal-adoption","recorded_at_unix_seconds":1})
        (root / "Control/relations/source-relations.json").write_text(json.dumps({
            "schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations}),encoding="utf-8")
        environment = dict(os.environ)
        environment["CENTRAL_NATIVE_TOKEN"] = TOKEN

        def argv(action, value):
            return [binary,"--json","--root",str(root),"action","run",action,json.dumps(value)]

        def invoke(action, value):
            # Regular temporary descriptors avoid inherited-pipe EOF dependency.
            # Timeout owns/reaps the single native ctrl child; this path invokes no provider.
            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                completed = subprocess.run(argv(action,value),env=environment,stdout=stdout,stderr=stderr,
                                           timeout=20,check=False)
                assert os.fstat(stdout.fileno()).st_size <= CAPTURE, "actual stdout exceeds proof capture profile"
                assert os.fstat(stderr.fileno()).st_size <= CAPTURE, "actual stderr exceeds proof capture profile"
                stdout.seek(0); stderr.seek(0)
                raw = stdout.read(); diagnostic = stderr.read().decode("utf-8",errors="strict")
                response = json.loads(raw)
                assert completed.returncode == 0 and response["ok"], (action,completed.returncode,response,diagnostic)
                return response["data"]

        admitted = invoke("central.work.policy",{})
        allocated = invoke("central.now.allocate",{"task_ref":"task:receiving-lost-ack-fixture",
            "purpose":"Actual native receipt lookup after transport loss","expected_policy_revision":admitted["revision"]})
        original = {"producer_key":"controlled-lost-ack-original","now_ref":allocated["now_ref"],
            "occurred_at_unix_seconds":42,"request":{"kind":"question","subject":"Did this actual receipt publish?"}}
        assert invoke("central.receiving.list",{})["returns"] == []
        # main executes run_cli_with_surface before println. The closed reader
        # loses that genuine output acknowledgement; no mocked owner response.
        reader, writer = os.pipe()
        os.close(reader)
        try:
            with tempfile.TemporaryFile() as stderr:
                lost = subprocess.run(argv("central.receiving.submit",original),env=environment,
                                      stdout=writer,stderr=stderr,timeout=20,check=False)
                assert os.fstat(stderr.fileno()).st_size <= CAPTURE
                stderr.seek(0); lost_diagnostic = stderr.read().decode("utf-8",errors="strict")
        finally:
            os.close(writer)
        assert lost.returncode != 0, "closed transport must not produce acknowledged semantic success"
        recovered = invoke("central.receiving.read",{"producer_key":original["producer_key"],"original_request":original})
        assert recovered["lookup"]["original_request_verified"] is True
        assert recovered["record"]["author"]["principal_ref"] == "agent:receiving-lost-ack-fixture"
        assert recovered["record"]["occurred_at_unix_seconds"] == 42
        area = root / ".central/source-returns/contributions"
        before = {p.name:(p.read_bytes(),p.stat().st_ino) for p in area.iterdir()}
        by_ref = invoke("central.receiving.read",{"return_ref":recovered["return_ref"]})
        repeated = invoke("central.receiving.read",{"producer_key":original["producer_key"],"original_request":original})
        assert by_ref["record"] == repeated["record"] == recovered["record"]
        assert by_ref["revision"] == repeated["revision"] == recovered["revision"]
        assert {p.name:(p.read_bytes(),p.stat().st_ino) for p in area.iterdir()} == before
        assert len(invoke("central.receiving.list",{})["returns"]) == 1
        report = {"schema":"central.receiving-lost-ack-native-proof/v1","native_binary":binary,
            "lost_transport_exit":lost.returncode,"lost_transport_diagnostic":lost_diagnostic,
            "submission_calls":1,"lookup_calls":2,"receipt_count":1,
            "recovered":recovered,"personal_installation":False,"world":"owned native scratch, deleted on return"}
    assert not root.exists(), "owned fixture did not clean up"
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl",required=True)
    arguments = parser.parse_args()
    print(json.dumps(run(arguments.ctrl),indent=2))
