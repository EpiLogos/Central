#!/usr/bin/env python3
"""Actual Central CLI acceptance for Flow disclosure and durable returned work."""
import os,subprocess,tempfile,pathlib,json,concurrent.futures
binary=os.environ['CTRL_BIN'];root=pathlib.Path(tempfile.mkdtemp(prefix='central-return-native-')).resolve();checks=[]
def check(v,n):assert v,n;checks.append(n)
def run(op,data):
 p=subprocess.run([binary,'--root',str(root),'--json','action','run',op,json.dumps(data)],capture_output=True,text=True,timeout=15)
 try:return json.loads(p.stdout)
 except Exception:raise RuntimeError((p.returncode,p.stdout,p.stderr))
def good(op,data):
 r=run(op,data);assert r['ok'],r;return r['data']
good('central.init',{});project=root/'Work'/'Proof';project.mkdir();good('projectcentral.init',{'project':'Proof','project_id':'source-return-proof'})
f=good('projectcentral.flow.create',{'project':'Proof','path':'notes/flow.md','actor':'test-human','actor_kind':'human'})['flow']
flow=f['flow_ref'];source=f['source_ref'];path=project/f['path'];path.write_text('original thought\n')
reading=good('projectcentral.flow.read',{'project':'Proof','flow_ref':flow});basis=reading['flow']['current_revision']
inspection=good('projectcentral.flow.inspect',{'project':'Proof','flow_ref':flow});check('content' not in inspection,'Flow inspect returns no source body');check(inspection['capabilities']['read']['available'],'Flow inspect discloses native read capability')
check(good('projectcentral.flow.read',{'project':'Proof','flow_ref':flow,'expected_revision':basis})['content']=='original thought\n','exact-revision Flow read preserves native bytes')
check(not run('projectcentral.flow.read',{'project':'Proof','flow_ref':flow,'expected_revision':'stale'})['ok'],'stale exact-revision Flow read refuses')
path.parent.joinpath('.no-agent-retrieval').write_text('')
check(not good('projectcentral.flow.inspect',{'project':'Proof','flow_ref':flow})['capabilities']['read']['available'],'excluded Flow inspection reports unavailable')
for op in ['read','history']:
 r=run('projectcentral.flow.'+op,{'project':'Proof','flow_ref':flow});check(not r['ok'] and r['status']=='unavailable_capability','excluded Flow '+op+' refuses disclosure')
path.parent.joinpath('.no-agent-retrieval').unlink()
path.write_bytes(b'x'*(4*1024*1024+1));check(not run('projectcentral.flow.read',{'project':'Proof','flow_ref':flow})['ok'],'oversized Flow body refuses')
path.write_bytes(b'\xff\x00');check(not run('projectcentral.flow.read',{'project':'Proof','flow_ref':flow})['ok'],'binary Flow refuses text projection')
path.write_text('original thought\n')
proposal={'project':'Proof','source_ref':source,'expected_revision':basis,'proposed_content':'returned improvement\n','reason':'real native return','evidence_refs':['evidence:native-test'],'agent_session_ref':'agent-session/native-return'}
r=good('projectcentral.source.return',proposal);ref=r['proposal']['return_ref'];check(r['proposal']['status']=='pending' and path.read_text()=='original thought\n','source return persists proposal without mutating source')
check(r['proposal']['basis_content']=='original thought\n','return preserves exact basis bytes')
check(r['acceptance']['available'],'collaborative Flow return exposes existing native agent authority')
path.write_text('concurrent human writing\n')
conflict=good('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref,'expected_revision':basis,'acceptance':'human-accepted','accepted_by_ref':'human:test'})
check(conflict['outcome']=='conflict' and path.read_text()=='concurrent human writing\n','return acceptance preserves concurrent source and reports conflict')
check(good('projectcentral.source.return_read',{'project':'Proof','return_ref':ref})['proposal']['status']=='pending','conflicted return stays pending')
current=good('projectcentral.flow.read',{'project':'Proof','flow_ref':flow})['flow']['current_revision']
r2=good('projectcentral.source.return',dict(proposal,expected_revision=current));ref2=r2['proposal']['return_ref']
accepted=good('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref2,'expected_revision':current,'acceptance':'human-accepted','accepted_by_ref':'human:test'})
check(accepted['outcome']=='accepted' and path.read_text()=='returned improvement\n','explicit collaborative return applies exact proposal through native source CAS')
check(accepted['proposal']['agent_session_ref']=='agent-session/native-return','accepted return retains native agent provenance')
history=good('projectcentral.flow.history',{'project':'Proof','flow_ref':flow})
check(history['revisions'][-1]['actor_kind']=='agent' and history['revisions'][-1]['agent_session_ref']=='agent-session/native-return','accepted Flow return preserves native Flow revision attribution')
check(not run('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref2,'expected_revision':current,'acceptance':'human-accepted','accepted_by_ref':'human:test'})['ok'],'already accepted return cannot apply twice')
page=good('projectcentral.source.returns',{'project':'Proof','limit':1});check(page['more'] and len(page['entries'])==1 and 'proposed_content' not in page['entries'][0],'return list bounded metadata page')
older=good('projectcentral.source.returns',{'project':'Proof','limit':1,'before':page['next_before']});check(len(older['entries'])==1 and not older['more'],'return list cursor continues without repeats')
rejected=good('projectcentral.source.return_reject',{'project':'Proof','return_ref':ref});check(rejected['proposal']['status']=='rejected' and path.read_text()=='returned improvement\n','rejection leaves source unchanged')
# Several native return candidates compete against the same source revision.
cb=good('projectcentral.flow.read',{'project':'Proof','flow_ref':flow})['flow']['current_revision']
competing=[good('projectcentral.source.return',dict(proposal,expected_revision=cb,proposed_content=f'candidate {n}'))['proposal']['return_ref'] for n in range(5)]
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
    outcomes=list(pool.map(lambda ref:good('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref,'expected_revision':cb,'acceptance':'human-accepted','accepted_by_ref':'human:test'}),competing))
check(sum(r['outcome']=='accepted' for r in outcomes)==1 and sum(r['outcome']=='conflict' for r in outcomes)==4,'five native Return acceptance processes serialize one winner and four conflicts')
# Authored human aperture: explicit acceptance text cannot grant authority.
human=project/'ProjectCentral/user/authored.md';human.write_text('human ground')
location=next(e['location'] for e in good('central.files.list',{'path':'Work/Proof/ProjectCentral/user'})['entries'] if e['name']=='authored.md')
hreading=good('central.files.read',{'location':location});hsource=hreading['source']['ref'];hbasis=hreading['revision']
hr=good('projectcentral.source.return',dict(proposal,source_ref=hsource,expected_revision=hbasis));check(not hr['acceptance']['available'],'authored return names missing native human authority')
refusal=run('projectcentral.source.return_accept',{'project':'Proof','return_ref':hr['proposal']['return_ref'],'expected_revision':hbasis,'acceptance':'human-accepted','accepted_by_ref':'human:claimed'})
check(not refusal['ok'] and refusal['status']=='unavailable_capability' and human.read_text()=='human ground','acceptance strings cannot overwrite authored human ground')
# A Flow adopted from authored ground must retain that authority too.
refused_adoption=run('projectcentral.flow.adopt',{'project':'Proof','path':'ProjectCentral/user/authored.md','actor':'test-human','actor_kind':'human'})
check(not refused_adoption['ok'] and human.read_text()=='human ground','Flow adoption refuses authored aperture overlap')
print(json.dumps({'ok':True,'count':len(checks),'checks':checks,'root':str(root),'binary':binary},indent=2))
