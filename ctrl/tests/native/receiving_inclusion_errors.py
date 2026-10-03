#!/usr/bin/env python3
"""Actual native inclusion error output; explicit qualified ctrl, owned native scratch.
These public CLI single-fault cases are distinct from the cfg(test) dual-fault gates.
No fake receipt/IO, production failure switch, provider, resubmit or personal install.
"""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile

CAPTURE = 1024 * 1024
HUMAN = "receiving-error-cli-controlled-human-not-a-real-secret"
AGENT = "receiving-error-cli-controlled-agent-not-a-real-secret"
BODY = "<p>actual private CLI inclusion contribution body</p>"


class RestoreMode:
    def __init__(self, path):
        self.path = path
        metadata = path.lstat()
        assert not stat.S_ISLNK(metadata.st_mode)
        self.identity = (metadata.st_dev, metadata.st_ino)
        self.mode = stat.S_IMODE(metadata.st_mode)

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, traceback):
        metadata = self.path.lstat()
        assert not stat.S_ISLNK(metadata.st_mode) and (metadata.st_dev, metadata.st_ino) == self.identity, (
            "owned permission fixture changed affiliation", str(self.path))
        self.path.chmod(self.mode)
        assert stat.S_IMODE(self.path.lstat().st_mode) == self.mode, ("fixture mode restoration failed", str(self.path))
        return False


def run(binary):
    assert os.geteuid() != 0, "actual directory EACCES prerequisite unavailable under root; must not pass"
    binary = str(Path(binary).resolve(strict=True))
    binary_sha256 = hashlib.sha256(Path(binary).read_bytes()).hexdigest()
    scratch = Path(__file__).resolve().parents[3] / "ProjectCentral/now/tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    cases = []
    with tempfile.TemporaryDirectory(prefix="receiving-error-cli-", dir=scratch) as owned:
        root = Path(owned)
        (root / "Control/user").mkdir(parents=True)
        (root / "Control/relations").mkdir()
        (root / "Work/demo").mkdir(parents=True)
        actions = ["central.document.create", "central.document.mutate", "central.receiving.submit",
                   "central.receiving.review", "central.receiving.include", "central.receiving.recover"]
        grants = [{"principal_ref":actor, "actor_kind":kind,
                   "token_sha256":hashlib.sha256(token.encode()).hexdigest(),
                   "scope_refs":["control:root"], "actions":actions, "expires_at_unix_seconds":9999999999}
                  for token, actor, kind in [(HUMAN,"human:receiving-error-cli-fixture","human"),
                                            (AGENT,"agent:receiving-error-cli-fixture","agent")]]
        policy = {"schema":"central.work-placement-policy/v1", "scope_ref":"control:root",
                  "writable":[{"path":"Work/demo", "class":"repository"}], "enforcement":"native-actions",
                  "required_coverage":["file-content"], "lease_seconds":300}
        time_policy = {"schema":"central.civil-time-policy/v1", "scope_ref":"control:root",
                       "timezone":"Europe/London", "day_boundary_minutes":0, "automatic_day_rollover":True}
        authority = {"schema":"central.native-action-authority/v1", "scope_ref":"control:root", "grants":grants}
        relations = []
        for name, role, value in [("placement.json","work-placement-policy",policy),
                                  ("time.json","civil-time-policy",time_policy),
                                  ("authority.json","native-action-authority",authority)]:
            member = "Control/user/" + name
            (root / member).write_text(json.dumps(value),encoding="utf-8")
            relations.append({"ref":"central:source:control:root:"+member, "path":member, "roles":[role],
                              "provenance":"human-adopted", "standing":"architecture-contract",
                              "treatment":"projectcentral-user", "recognition":"controlled-native-fixture-not-personal-adoption",
                              "recorded_at_unix_seconds":1})
        (root / "Control/relations/source-relations.json").write_text(json.dumps({
            "schema":"central.control.ground-relations/v1", "project_id":"control:root", "relations":relations}),encoding="utf-8")

        def invoke(action, value, token=HUMAN, failure=False, ordinary_refusal=False):
            environment = dict(os.environ)
            environment["CENTRAL_NATIVE_TOKEN"] = token
            argv = [binary,"--json","--root",str(root),"action","run",action,json.dumps(value)]
            # SAME existing finite native proof mechanics: regular capture files
            # avoid inherited pipe EOF dependence; no model/provider is launched.
            with tempfile.TemporaryFile(dir=root) as stdout, tempfile.TemporaryFile(dir=root) as stderr:
                completed = subprocess.run(argv,env=environment,stdout=stdout,stderr=stderr,timeout=20,check=False)
                assert os.fstat(stdout.fileno()).st_size <= CAPTURE, "native stdout exceeds proof profile"
                assert os.fstat(stderr.fileno()).st_size <= CAPTURE, "native stderr exceeds proof profile"
                stdout.seek(0); stderr.seek(0)
                response = json.loads(stdout.read())
                diagnostic = stderr.read().decode("utf-8",errors="strict")
                if ordinary_refusal:
                    assert completed.returncode == 4 and response["ok"] is False, (action,completed.returncode,response,diagnostic)
                    assert response["status"] == "unavailable_capability"
                    assert response["error"]["code"] == "policy_or_source_denied"
                    assert "receiving" not in response["error"]["details"]
                    return response
                if failure:
                    assert completed.returncode == 6 and response["ok"] is False, (action,completed.returncode,response,diagnostic)
                    assert response["status"] == "partial_completion"
                    assert response["error"]["code"] == "central.receiving.inclusion_incomplete"
                    return response
                assert completed.returncode == 0 and response["ok"], (action,completed.returncode,response,diagnostic)
                return response["data"]

        for label in ["directory-os", "readonly-preflight"]:
            policy_reading = invoke("central.work.policy",{})
            document = invoke("central.document.create",{"kind":"flow","document_id":"doc:"+label,
                "title":"", "fields":[], "expected_policy_revision":policy_reading["revision"]})
            original = {"producer_key":"producer:"+label, "source_ref":document["source"]["ref"],
                        "document_id":document["document_id"], "expected_source_revision":document["revision"]["revision"],
                        "occurred_at_unix_seconds":42, "proposal":{"operation":"entry.add","entry_id":"entry:"+label,
                        "contribution_id":"part:"+label, "html":BODY}}
            submitted = invoke("central.receiving.submit",original,AGENT)
            before_review = invoke("central.receiving.read",{"return_ref":submitted["return_ref"]},AGENT)
            invoke("central.receiving.include",{"return_ref":submitted["return_ref"],
                "expected_return_revision":submitted["revision"],"expected_source_revision":document["revision"]["revision"]},
                ordinary_refusal=True)
            assert invoke("central.receiving.read",{"return_ref":submitted["return_ref"]},AGENT) == before_review
            accepted = invoke("central.receiving.review",{"return_ref":submitted["return_ref"],
                "expected_return_revision":submitted["revision"],"expected_source_revision":document["revision"]["revision"],
                "disposition":"accepted"})
            target = root / document["source"]["path"]
            source_before = target.read_bytes(); source_metadata = target.lstat()
            guarded_path = target.parent if label == "directory-os" else target
            with RestoreMode(guarded_path):
                guarded_path.chmod(0o555 if label == "directory-os" else 0o444)
                actual_errno = None
                if label == "directory-os":
                    oracle_path = target.parent / ".receiving-cli-unused-os-oracle"
                    assert not oracle_path.exists()
                    try:
                        with oracle_path.open("xb"):
                            raise AssertionError("actual nonroot parent unexpectedly allowed creation")
                    except PermissionError as observed:
                        actual_errno = observed.errno
                    assert actual_errno == errno.EACCES and not oracle_path.exists()
                failed = invoke("central.receiving.include",{"return_ref":submitted["return_ref"],
                    "expected_return_revision":accepted["revision"],"expected_source_revision":document["revision"]["revision"]},failure=True)
                details = failed["error"]["details"]
                encoded = json.dumps(details)
                for private in [BODY,HUMAN,AGENT]:
                    assert private not in encoded, "actual body/credential leaked through added native details"
                assert len(json.dumps(details,separators=(",",":"),ensure_ascii=False).encode("utf-8")) <= 64*1024
                assert details["causes"]["document"]["kind"] == "PermissionDenied"
                assert details["causes"]["document"]["raw_os_error"] == actual_errno
                assert details["causes"]["receiving_write"] is None
                assert details["document"]["outcome"] == "unconfirmed"
                assert details["document"]["operation_receipt"] is None
                assert details["receiving"]["prior_acknowledged"]["status"] == "including"
                assert details["receiving"]["attempted_status"] == "uncertain"
                assert details["receiving"]["persistence"] == "acknowledged"
                assert details["automatic_retry"] is False
                area = root / ".central/source-returns/contributions"
                before = {path.name:(path.read_bytes(),path.lstat().st_ino) for path in area.iterdir()}
                by_ref = invoke("central.receiving.read",{"return_ref":submitted["return_ref"]},AGENT)
                lookup = invoke("central.receiving.read",{"producer_key":original["producer_key"],"original_request":original},AGENT)
                assert lookup["lookup"]["original_request_verified"] is True
                assert by_ref["record"] == lookup["record"] and by_ref["revision"] == lookup["revision"]
                assert lookup["record"]["status"] == "uncertain" and lookup["included"] is False
                assert details["receiving"]["update_acknowledged"] == {"status":"uncertain","revision":lookup["revision"]}
                assert details["receiving"]["prior_acknowledged"]["revision"] != lookup["revision"]
                for field in ["schema","return_ref","scope_ref","sequence","request_digest","proposal","author",
                              "authority_ref","authority_revision","occurred_at_unix_seconds","received_at_unix_seconds"]:
                    assert lookup["record"][field] == submitted["record"][field], field
                assert {path.name:(path.read_bytes(),path.lstat().st_ino) for path in area.iterdir()} == before
                assert target.read_bytes() == source_before
                after = target.lstat(); assert (after.st_dev,after.st_ino) == (source_metadata.st_dev,source_metadata.st_ino)
                assert invoke("central.document.read",{"source_ref":document["source"]["ref"],
                    "document_id":document["document_id"]})["document"] == document["document"]
                cases.append({"case":label,"native_kind":details["causes"]["document"]["kind"],
                              "actual_raw_os_error":actual_errno,"current_receiving_status":lookup["record"]["status"],
                              "submission_calls":1,"automatic_retry":False})
            assert target.read_bytes() == source_before
        assert len(invoke("central.receiving.list",{})["returns"]) == 2
    assert not root.exists(), "owned native CLI fixture cleanup failed"
    return {"schema":"central.receiving-inclusion-error-native-proof/v1","native_binary":binary,
            "native_binary_sha256":binary_sha256,"cases":cases,"dual_error_cli_proof":False,
            "world":"owned native scratch removed","personal_installation":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl",required=True)
    arguments = parser.parse_args()
    print(json.dumps(run(arguments.ctrl),indent=2))
