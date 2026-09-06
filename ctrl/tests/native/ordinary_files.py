#!/usr/bin/env python3
"""Real CLI/filesystem acceptance; no backend substitutes. CTRL_BIN selects candidate."""
import concurrent.futures, json, os, pathlib, subprocess, tempfile, sys, threading, time
BIN=os.environ['CTRL_BIN']
root=pathlib.Path(tempfile.mkdtemp(prefix='central-file-native-')).resolve()
checks=[]
def check(value,name):
    assert value,name
    checks.append(name)
def run(op,data=None):
    args=[BIN,'--root',str(root),'--json']
    args+=['action','run','central.files.'+op,json.dumps(data or {})] if op!='init' else ['init']
    p=subprocess.run(args,capture_output=True,text=True)
    try:return json.loads(p.stdout)
    except Exception:raise RuntimeError((args,p.returncode,p.stdout,p.stderr))
def good(op,data):
    out=run(op,data);assert out['ok'],out
    return out['data']
run('init'); folder=root/'Work'/'Bare';folder.mkdir(parents=True)
file=folder/'note.txt';file.write_text('before\n');file.chmod(0o640)
loc=next(e['location'] for e in good('list',{'path':'Work/Bare'})['entries'] if e['name']=='note.txt')
reading=good('read',{'location':loc});basis=reading['revision']
check(reading['operations']['write']['available'],'native owner discloses ordinary write availability')
check(reading['project']['project_ref'] is None,'unadopted directory stays unadopted')
empty=good('history',{'location':loc});check(empty['entries']==[],'initial history empty')
check(not (root/'.central/file-history').exists(),'initial history read creates no owner state')
base={'location':loc,'expected_revision':basis,'content':'after\n','actor':'native-test','actor_kind':'human'}
written=good('write',base);check(written['outcome']=='written' and file.read_text()=='after\n','real ordinary CAS writes bytes')
check(file.stat().st_mode&0o777==0o640,'atomic replacement preserves mode')
check(not (folder/'ProjectCentral').exists(),'write does not adopt project')
conflict=good('write',base);check(conflict['outcome']=='conflict' and conflict['current']['content']=='after\n','stale CAS returns live owner reading')
check(file.read_text()=='after\n','conflict preserves concurrent bytes')
history=good('history',{'location':loc,'limit':1});check(history['entries'][0]['previous_revision']==basis,'history records exact previous revision')
preview=good('recovery_preview',{'location':loc,'expected_revision':written['revision'],'revision':basis});check(preview['content']=='before\n' and file.read_text()=='after\n','recovery preview preserves file')
restored=good('restore',dict(base,expected_revision=written['revision'],revision=basis));check(restored['revision']==basis and file.read_text()=='before\n','restore commits historical bytes by CAS')
page=good('history',{'location':loc,'limit':1});check(page['more'] and page['next_before']==2,'history bounded newest-first cursor')
older=good('history',{'location':loc,'limit':1,'before':page['next_before']});check(older['entries'][0]['cursor']==1 and not older['more'],'history continuation exact no repeats')
with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
    results=list(pool.map(lambda n:good('write',dict(base,content=f'writer {n}\n')),range(8)))
check(sum(x['outcome']=='written' for x in results)==1,'eight native processes serialize one successful same-basis commit')
check(sum(x['outcome']=='conflict' for x in results)==7,'seven concurrent writers receive conflicts')
current=good('read',{'location':loc})
check(not run('write',dict(base,expected_revision=current['revision'],agent_session_ref='agent-session/test'))['ok'],'human with agent-session attribution refused')
# Exact root identity and component validation.
other=dict(loc,root='/tmp/other')
check(not run('write',dict(base,location=other))['ok'],'root identity mismatch refused')
traversal=dict(loc,path='../outside')
check(not run('write',dict(base,location=traversal))['ok'],'traversal refused')
outside=root.parent/(root.name+'-outside');outside.write_text('private')
file.unlink();file.symlink_to(outside)
check(not run('write',dict(base,expected_revision=current['revision']))['ok'] and outside.read_text()=='private','leaf symlink refuses mutation outside root')
file.unlink();file.write_text('again')
moved=root/'moved';folder.rename(moved);folder.symlink_to(moved,target_is_directory=True)
check(not run('write',base)['ok'],'ancestor symlink refused')
folder.unlink();moved.rename(folder)
(root/'Work/Bare/.no-agent-retrieval').write_text('')
check(not run('write',base)['ok'],'retrieval-excluded write refused')
(root/'Work/Bare/.no-agent-retrieval').unlink()
# Protected ground has no ordinary route even with claimed human attribution.
protected=root/'Control/user/secret.md';protected.parent.mkdir(parents=True,exist_ok=True);protected.write_text('human')
ploc=next(e['location'] for e in good('list',{'path':'Control/user'})['entries'] if e['name']=='secret.md')
check(not good('read',{'location':ploc})['operations']['write']['available'],'native owner discloses protected route unavailability')
for kind in ['human','agent','system']:
    denied=run('write',dict(base,location=ploc,actor_kind=kind));check(not denied['ok'] and denied['error']['details']['outcome']=='refused',f'protected ground refuses {kind} ordinary bypass')
# Unicode, spaces, and preserved extended metadata exercise actual OS paths.
u=folder/'space — %.txt';u.write_text('unicode');subprocess.run(['/usr/bin/xattr','-w','org.central.test','preserved',str(u)],check=True) if sys.platform=='darwin' else os.setxattr(u,b'user.central-test',b'preserved')
uloc=next(e['location'] for e in good('list',{'path':'Work/Bare'})['entries'] if e['name']==u.name)
ur=good('read',{'location':uloc});good('write',dict(base,location=uloc,expected_revision=ur['revision']))
check(u.read_text()=='after\n','unicode delimiter path commits exact target')
if sys.platform=='darwin':check(subprocess.check_output(['/usr/bin/xattr','-p','org.central.test',str(u)]).strip()==b'preserved','macOS atomic write preserves extended attributes')
# Real participating-source initialization proves identity cannot be demoted.
(folder/'README.md').write_text('participating source')
p=subprocess.run([BIN,'--root',str(root),'--json','action','run','projectcentral.init',json.dumps({'project':'Bare','project_id':'ordinary-files-acceptance'})],capture_output=True,text=True)
assert json.loads(p.stdout)['ok'],p.stdout
source=folder/'ProjectCentral/user/authored.md';source.write_text('participating source')
sloc=next(e['location'] for e in good('list',{'path':'Work/Bare/ProjectCentral/user'})['entries'] if e['name']=='authored.md')
sread=good('read',{'location':sloc})
check(sread['source'] is not None,'participating authored file keeps native SourceBinding')
refused=run('write',dict(base,location=sloc,expected_revision=sread['revision']))
check(not refused['ok'] and source.read_text()=='participating source','participating SourceRef refuses ordinary bypass')
# Invalid project identity is not interpreted as an unadopted directory.
manifest=folder/'ProjectCentral'/'project.json';saved=manifest.read_bytes();manifest.write_text('invalid JSON')
check(not run('write',dict(base,expected_revision=good('read',{'location':loc})['revision']))['ok'],'invalid adopted project fails closed')
manifest.write_bytes(saved)
# Actual parent symlink churn while separate CLI processes attempt commits.
race=root/'race';race.mkdir();(race/'note').write_text('basis')
external=pathlib.Path(tempfile.mkdtemp(prefix='central-outside-')).resolve();(external/'note').write_text('basis')
rloc=good('list',{'path':'race'})['entries'][0]['location'];rread=good('read',{'location':rloc})
stop=threading.Event()
def churn():
    held=root/'race-held'
    while not stop.is_set():
        try:
            race.rename(held);race.symlink_to(external,target_is_directory=True)
            time.sleep(.0001);race.unlink();held.rename(race)
        except FileNotFoundError: pass
    if race.is_symlink():race.unlink()
    if held.exists() and not race.exists():held.rename(race)
t=threading.Thread(target=churn);t.start()
try:
    for _ in range(30):run('write',dict(base,location=rloc,expected_revision=rread['revision'],content='inside'))
finally:stop.set();t.join()
check((external/'note').read_text()=='basis','ancestor symlink churn never mutates external sentinel')
# Kill an actual native writer after its durable prepare record appears. The
# next owner history read reconciles committed vs not-committed bytes.
k=root/'kill-test';k.mkdir();(k/'note').write_text('kill basis')
kloc=good('list',{'path':'kill-test'})['entries'][0]['location'];kr=good('read',{'location':kloc})
request=dict(base,location=kloc,expected_revision=kr['revision'],content='z'*60000)
proc=subprocess.Popen([BIN,'--root',str(root),'--json','action','run','central.files.write',json.dumps(request)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
saw=False
while proc.poll() is None:
    pending=list((root/'.central/file-history').glob('*/pending.json'))
    if pending:
        saw=True;proc.kill();break
proc.communicate()
check(saw,'observed durable prepare before terminating actual native process')
kh=good('history',{'location':kloc})
if (k/'note').read_text()==request['content']:
    check(any(e['previous_revision']==kr['revision'] for e in kh['entries']),'committed process interruption preserves recovery receipt')
else:
    check((k/'note').read_text()=='kill basis','interrupted precommit preserves original bytes')
check(not list((root/'.central/file-history').glob('*/pending.json')),'native history reconciles interrupted pending receipt')
print(json.dumps({'ok':True,'checks':checks,'count':len(checks),'root':str(root),'binary':BIN},indent=2))
