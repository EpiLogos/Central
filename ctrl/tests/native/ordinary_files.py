#!/usr/bin/env python3
"""Real CLI/filesystem acceptance; no backend substitutes. CTRL_BIN selects candidate."""
import concurrent.futures, hashlib, json, os, pathlib, select, shutil, signal, subprocess, tempfile, sys, threading, time
BIN=os.environ['CTRL_BIN']
scratch=pathlib.Path(__file__).resolve().parents[3]/'ProjectCentral'/'now'/'tmp'
scratch.mkdir(parents=True,exist_ok=True)
owned=pathlib.Path(tempfile.mkdtemp(prefix='central-file-native-',dir=scratch)).resolve()
root=owned/'Central';root.mkdir()
# Failed native activity retains its owned T fixture as evidence. Cleanup occurs
# only after every real child is reaped and every original assertion succeeds.
checks=[]
def check(value,name):
    assert value,name
    checks.append(name)
def run(op,data=None,timeout=15,evidence=None):
    args=[BIN,'--root',str(root),'--json']
    args+=['action','run','central.files.'+op,json.dumps(data or {})] if op!='init' else ['init']
    try:p=subprocess.run(args,capture_output=True,timeout=timeout)
    except subprocess.TimeoutExpired as failure:
        if evidence is not None:
            evidence.with_suffix(".stdout").write_bytes(failure.output or b'')
            evidence.with_suffix(".stderr").write_bytes(failure.stderr or b'')
            evidence.with_suffix(".account.json").write_text(json.dumps({"argv":args,"returncode":None,"capture_completed":False,"timeout":timeout,"Original_development_Run_credit":False},indent=2)+"\n")
        raise
    if evidence is not None:
        evidence.with_suffix(".stdout").write_bytes(p.stdout)
        evidence.with_suffix(".stderr").write_bytes(p.stderr)
        evidence.with_suffix(".account.json").write_text(json.dumps({"argv":args,"returncode":p.returncode,"capture_completed":True,"Original_development_Run_credit":False},indent=2)+"\n")
    try:return json.loads(p.stdout)
    except Exception:raise RuntimeError((args,p.returncode,p.stdout,p.stderr))
def good(op,data,timeout=15,evidence=None):
    out=run(op,data,timeout=timeout,evidence=evidence);assert out['ok'],out
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
# The ratified flow-instance creation door: an absent file under
# Control/user/flows/ is created by an explicit write with an empty
# expected revision; everywhere else absent files are refused.
flowloc={'schema':'central.path-ref/v1','ref':f"central:path:{root}:Control/user/flows/flow-2026-09-13-1200.html",'root':str(root),'path':'Control/user/flows/flow-2026-09-13-1200.html'}
created=good('write',{'location':flowloc,'expected_revision':'','content':'<p>flow instance</p>','actor':'native-test','actor_kind':'human'})
check(created['outcome']=='created' and (root/'Control/user/flows/flow-2026-09-13-1200.html').read_text()=='<p>flow instance</p>','absent flow instance created with journal-seeded revision')
check(good('read',{'location':flowloc})['revision']==created['revision'],'created revision is the live owner revision')
refused=run('write',{'location':{'schema':'central.path-ref/v1','ref':f"central:path:{root}:Control/agents/now/flows/x.md",'root':str(root),'path':'Control/agents/now/flows/x.md'},'expected_revision':'','content':'x','actor':'native-test','actor_kind':'human'})
check(not refused['ok'] and not (root/'Control/agents/now/flows/x.md').exists(),'creation outside Control/user/flows is refused and creates nothing')
claimed=run('write',{'location':flowloc,'expected_revision':'','content':'second','actor':'native-test','actor_kind':'human'})
check(claimed['data']['outcome']=='conflict' and (root/'Control/user/flows/flow-2026-09-13-1200.html').read_text()=='<p>flow instance</p>','a second creation is a conflict; original bytes preserved')
bumped=good('write',{'location':flowloc,'expected_revision':created['revision'],'content':'<p>flow instance, revised</p>','actor':'native-test','actor_kind':'human'})
check(bumped['outcome']=='written' and bumped['revision']!=created['revision'],'the created instance then writes like any ordinary file')

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
u=folder/'space — %.txt';u.write_text('unicode');subprocess.run(['/usr/bin/xattr','-w','org.central.test','preserved',str(u)],check=True,timeout=5) if sys.platform=='darwin' else os.setxattr(u,b'user.central-test',b'preserved')
uloc=next(e['location'] for e in good('list',{'path':'Work/Bare'})['entries'] if e['name']==u.name)
ur=good('read',{'location':uloc});good('write',dict(base,location=uloc,expected_revision=ur['revision']))
check(u.read_text()=='after\n','unicode delimiter path commits exact target')
if sys.platform=='darwin':check(subprocess.check_output(['/usr/bin/xattr','-p','org.central.test',str(u)],timeout=5).strip()==b'preserved','macOS atomic write preserves extended attributes')
# Real participating-source initialization proves identity cannot be demoted.
(folder/'README.md').write_text('participating source')
p=subprocess.run([BIN,'--root',str(root),'--json','action','run','projectcentral.init',json.dumps({'project':'Bare','project_id':'ordinary-files-acceptance'})],capture_output=True,text=True,timeout=15)
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
external=owned/'unselected-outside';external.mkdir();(external/'note').write_text('basis')
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
finally:
    stop.set();t.join(timeout=5)
    assert not t.is_alive(),'actual owned churn worker failed to quiesce; fixture retained'
check((external/'note').read_text()=='basis','ancestor symlink churn never mutates external sentinel')
# Deterministic actual-native interruption. The explicitly selected test child
# includes the same CLI/owner Source; the production/default binary has no arm.
# Only the native owner publishes pending/events. An inherited one-byte pipe
# observes its exact bounded physical checkpoint without polling a short window.
INTERRUPTION_BIN=os.environ['CTRL_ORDINARY_INTERRUPTION_BIN']
checkpoint_evidence=pathlib.Path(os.environ['CTRL_NATIVE_EVIDENCE_DIR']).resolve(strict=True)/owned.name
checkpoint_evidence.mkdir(exist_ok=False)
interruption_cases=[];failed_arms=[]
control=root/'.ordinary-interruption-admission.json'
def native_interruption(name,after_rename,changed_source=False):
    k=root/name;k.mkdir();source=k/'note';source.write_text('kill basis')
    kloc=good('list',{'path':name})['entries'][0]['location'];kr=good('read',{'location':kloc})
    request=dict(base,location=kloc,expected_revision=kr['revision'],content='z'*60000)
    request_bytes=json.dumps(request).encode();rm=root.stat();sm=source.stat()
    admission={'schema':'central.native-ordinary-interruption-admission/v1','after_rename':after_rename,
        'root_device':rm.st_dev,'root_inode':rm.st_ino,'source_device':sm.st_dev,'source_inode':sm.st_ino,
        'request_sha256':hashlib.sha256(request_bytes).hexdigest()}
    control.write_text(json.dumps(admission))
    reader,writer=os.pipe();os.set_blocking(reader,False)
    env=os.environ.copy();env['CENTRAL_NATIVE_INTERRUPTION_NOTIFY_FD']=str(writer)
    argv=[INTERRUPTION_BIN,'--root',str(root),'--json','action','run','central.files.write',request_bytes.decode()]
    proc=None;marker=b'';pending=None;raw_pending=None;stdout=b'';stderr=b'';observed=False;drained=False;cleanup_error=None
    start=time.monotonic();deadline=start+5
    try:
        proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,pass_fds=(writer,))
        os.close(writer);writer=None
        while time.monotonic()<deadline:
            if proc.poll() is not None:break
            ready,_,_=select.select([reader],[],[],min(.02,max(0,deadline-time.monotonic())))
            if ready:
                marker=os.read(reader,2)
                if marker:break
        if marker:
            check(marker==(b'C' if after_rename else b'P'),'native checkpoint identifies actual selected publication phase')
            check(proc.poll() is None,'native checkpoint retains the actual live direct writer')
            identities=[identity for identity in (root/'.central/file-history').glob('*/identity.json') if json.loads(identity.read_text())==kloc]
            check(len(identities)==1,'interrupted native identity names exactly the selected ordinary file')
            pending=identities[0].parent/'pending.json';raw_pending=pending.read_bytes();event=json.loads(raw_pending)
            check(event['previous_revision']==kr['revision'],'durable interrupted pending retains exact original basis')
            remaining=deadline-time.monotonic()
            check(remaining>0,'native phase read begins within original five-second deadline')
            current=good('read',{'location':kloc},timeout=remaining,evidence=checkpoint_evidence/(name+'-phase-read'))
            check(current['revision']==(event['revision'] if after_rename else kr['revision']),'native source revision agrees with actual checkpoint phase')
            check(source.read_text()==(request['content'] if after_rename else 'kill basis'),'actual checkpoint preserves complete selected source bytes')
            check(time.monotonic()<deadline and proc.poll() is None,'native checkpoint observations retain live writer within original five-second deadline')
            observed=True
        if proc.poll() is None:proc.kill()
        stdout,stderr=proc.communicate(timeout=5);drained=True
        check(len(stdout)+len(stderr)<=1024*1024,'interrupted native output fits measured one-MiB profile')
    finally:
        if writer is not None:os.close(writer)
        os.close(reader)
        if proc is not None:
            if not drained:
                if proc.poll() is None:proc.kill()
                try:stdout,stderr=proc.communicate(timeout=2);drained=True
                except subprocess.TimeoutExpired as failure:
                    stdout=failure.output or stdout;stderr=failure.stderr or stderr;cleanup_error=str(failure)
            (checkpoint_evidence/(name+'.stdout')).write_bytes(stdout)
            (checkpoint_evidence/(name+'.stderr')).write_bytes(stderr)
            if raw_pending is not None:(checkpoint_evidence/(name+'.pending.json')).write_bytes(raw_pending)
            (checkpoint_evidence/(name+'.account.json')).write_text(json.dumps({'argv':argv,
                'returncode':proc.returncode,'direct_child_reaped':proc.poll() is not None,
                'stdout_eof':drained,'stderr_eof':drained,'cleanup_error':cleanup_error,'checkpoint_observed':observed,
                'checkpoint_byte_hex':marker.hex(),'elapsed_seconds':time.monotonic()-start,
                'Original_development_Run_credit':False},indent=2)+'\n')
            assert proc.poll() is not None and drained,'owned native writer must be reaped and both streams drained before fixture cleanup'
    check(observed,'observed durable prepare before terminating actual native process')
    check(proc.returncode==-signal.SIGKILL,'actual owned writer was killed rather than completing or reporting fixture timeout')
    if changed_source:
        source.write_text('later independent external bytes')
        refusal=run('history',{'location':kloc},evidence=checkpoint_evidence/(name+'-unresolved-recovery'))
        check(not refusal['ok'] and refusal['action']=='central.files.history'
            and refusal['status']=='internal_failure' and refusal['error']['code']=='internal_failure'
            and refusal['error']['details']['outcome']=='error'
            and refusal['error']['message']=='File commit has an unresolved interrupted receipt; current bytes match neither journal basis nor target. Owner recovery is required; do not resend.',
            'changed-neither interrupted receipt remains the exact native recovery failure')
        check(pending.read_bytes()==raw_pending and source.read_text()=='later independent external bytes','failed owner recovery retains exact pending and later source bytes')
        check(not list(pending.parent.glob('event-*.json')),'changed-neither failure cannot invent a committed event')
        source.write_text('kill basis') # explicit external fixture restoration of recorded basis
    kh=good('history',{'location':kloc},evidence=checkpoint_evidence/(name+'-owner-recovery'))
    if after_rename:
        check(source.read_text()==request['content'] and len(kh['entries'])==1 and kh['entries'][0]==event,
            'committed process interruption preserves recovery receipt')
    else:
        check(source.read_text()=='kill basis','interrupted precommit preserves original bytes')
        check(kh['entries']==[],'precommit interruption does not create a successful event')
    check(not pending.exists(),'native history reconciles interrupted pending receipt')
    check(good('history',{'location':kloc},evidence=checkpoint_evidence/(name+'-owner-replay'))==kh,'repeated owner recovery retains exact outcome without duplicate event')
    control.unlink()
    interruption_cases.append(name)
    return argv
# Missing/mismatched admission must fail before any owner mutation or checkpoint.
k=root/'interruption-refusal';k.mkdir();(k/'note').write_text('admission basis')
kloc=good('list',{'path':'interruption-refusal'})['entries'][0]['location'];kr=good('read',{'location':kloc})
request=dict(base,location=kloc,expected_revision=kr['revision'],content='refused target')
for admitted in (False,True):
    if admitted:control.write_text(json.dumps({'schema':'central.native-ordinary-interruption-admission/v1','after_rename':False,'root_device':root.stat().st_dev,'root_inode':root.stat().st_ino,'source_device':(k/'note').stat().st_dev,'source_inode':(k/'note').stat().st_ino,'request_sha256':'different request'}))
    refusal=subprocess.run([INTERRUPTION_BIN,'--root',str(root),'--json','action','run','central.files.write',json.dumps(request)],capture_output=True,timeout=5)
    arm_name='changed-admission' if admitted else 'missing-admission'
    (checkpoint_evidence/(arm_name+'.stdout')).write_bytes(refusal.stdout)
    (checkpoint_evidence/(arm_name+'.stderr')).write_bytes(refusal.stderr)
    (checkpoint_evidence/(arm_name+'.account.json')).write_text(json.dumps({'argv':refusal.args,'returncode':refusal.returncode,'capture_completed':True,'Original_development_Run_credit':False},indent=2)+'\n')
    check(refusal.returncode==78 and b'admission refused' in refusal.stderr and not refusal.stdout,'missing or changed admission refuses native test child before dispatch')
    check((k/'note').read_text()=='admission basis' and good('history',{'location':kloc})['entries']==[],'failed interruption arm leaves actual source and owner history unchanged')
    if admitted:control.unlink()
    failed_arms.append(arm_name)
native_interruption('kill-test',False)
native_interruption('kill-test-committed',True)
native_interruption('kill-test-changed',False,True)
check(not list((root/'.central/file-history').glob('*/pending.json')),'native history reconciles interrupted pending receipt')
check(interruption_cases==['kill-test','kill-test-committed','kill-test-changed'] and failed_arms==['missing-admission','changed-admission'],'every required actual checkpoint and failed-arm case completed')
(checkpoint_evidence/'completed-cases.json').write_text(json.dumps({'actual_completed_cases':interruption_cases,'actual_failed_arm_checks':failed_arms,'native_capture_and_recovery_artifacts_retained':True,'Original_development_Run_credit':False},indent=2)+'\n')
shutil.rmtree(owned)
check(not owned.exists(),'successful actual fixture removes only its owned outer')
print(json.dumps({'ok':True,'checks':checks,'count':len(checks),'root':str(root),'binary':BIN},indent=2))
