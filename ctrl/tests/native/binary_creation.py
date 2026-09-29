#!/usr/bin/env python3
"""Actual native process admission/recovery; no substitute owner responses."""
import base64
import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import tempfile

BINARY = os.environ['CTRL_BIN']
checks = []

def check(value, message):
    assert value, message
    checks.append(message)

with tempfile.TemporaryDirectory(prefix='central-binary-admission-') as directory:
    root = Path(directory).resolve()
    def run(action, data):
        result = subprocess.run([BINARY, '--root', str(root), '--json', 'action', 'run', action, '-'],
                                input=json.dumps(data), text=True, capture_output=True, timeout=30)
        try:
            return json.loads(result.stdout)
        except ValueError:
            raise AssertionError((result.returncode, result.stdout, result.stderr))
    def good(action, data):
        reply = run(action, data)
        assert reply['ok'], reply
        return reply['data']
    good('central.init', {})
    (root / 'images').mkdir()
    parent = good('central.files.list', {'path':'images'})['location']
    payload = bytes(range(256)) * 1024
    encoded = base64.b64encode(payload).decode()
    creation = dict(parent=parent, name='capture.bin', content=encoded, content_encoding='base64',
                    expected_absent=True, operation_ref='native:binary:first-save', actor='native-test', actor_kind='human')
    first = good('central.files.create', creation)
    loc = first['location']
    file = root / 'images/capture.bin'
    check(file.read_bytes() == payload, '256KiB stdin admission writes exact binary bytes beyond argv limit')
    check(file.stat().st_mode & 0o777 == 0o600, 'admission keeps private file permissions')
    read = good('central.files.read', dict(location=loc, encoding='base64'))
    check(not read['operations']['write']['available'] and read['operations']['history']['available'] and read['operations']['restore']['available'], 'binary disclosure distinguishes refused text writes from native recovery')
    check(read['revision'] == first['revision'] and read['content'] == encoded, 'independent process reads exact owner revision and bytes')
    check(good('central.files.create', creation)['outcome'] == 'unchanged', 'same operation replays idempotently')
    check(not run('central.files.create', dict(creation, operation_ref='native:other'))['ok'], 'different operation cannot overwrite')
    check(not run('central.files.read', dict(location=loc))['ok'], 'binary file cannot masquerade as UTF-8')
    history = good('central.files.history', dict(location=loc))
    check(history['entries'][0]['revision'] == first['revision'], 'creation seeds native recovery history')
    external = b'\0actual external editor\xff'
    file.write_bytes(external)
    current = good('central.files.read', dict(location=loc, encoding='base64'))
    request = dict(location=loc, expected_revision=current['revision'], revision=first['revision'], actor='native-test', actor_kind='human')
    preview = good('central.files.recovery_preview', request)
    check(preview['content_encoding'] == 'base64' and base64.b64decode(preview['content']) == payload and file.read_bytes() == external, 'preview discloses exact historical bytes without write')
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        outcomes = list(pool.map(lambda _: good('central.files.restore', request), range(4)))
    check(sum(x['outcome'] == 'written' for x in outcomes) == 1 and sum(x['outcome'] == 'conflict' for x in outcomes) == 3, 'four real processes commit exactly one same-basis binary restore')
    check(file.read_bytes() == payload, 'restore retains exact original bytes after process restart')
    check(good('central.files.history', dict(location=loc))['entries'][0]['restored_from'] == first['revision'], 'restoration has native lineage receipt')
    for content, encoding in [('!', 'base64'), ('AA==', 'unknown'), (base64.b64encode(bytes(4*1024*1024+1)).decode(), 'base64')]:
        refused = run('central.files.create', dict(creation, name='refused.bin', content=content, content_encoding=encoding))
        check(not refused['ok'] and not (root/'images/refused.bin').exists(), 'invalid or oversized encoding refuses before admission')
    (root/'images/.no-agent-retrieval').touch()
    check(not run('central.files.create', dict(creation, name='private.bin'))['ok'] and not (root/'images/private.bin').exists(), 'retrieval exclusion refuses before admission')
    check(not run('central.files.restore', request)['ok'], 'recovery cannot bypass retrieval exclusion')
    (root/'images/.no-agent-retrieval').unlink()
    (root/'images/link.bin').symlink_to(file)
    check(not run('central.files.create', dict(creation, name='link.bin'))['ok'] and file.read_bytes() == payload, 'symlink destination cannot replace an existing native file')
    protected = good('central.files.list', {'path':'Control/user'})['location']
    check(not run('central.files.create', dict(creation, parent=protected, name='forbidden.bin'))['ok'], 'protected ground retains native admission refusal')
print(json.dumps({'passed':len(checks), 'checks':checks}, indent=2))
