#!/usr/bin/env python3
"""Actual Central CLI acceptance for durable returned work over ordinary world sources."""
import os,subprocess,tempfile,pathlib,json,concurrent.futures,sys,stat
native_scratch=pathlib.Path(__file__).resolve().parents[3]/'ProjectCentral/now/tmp';native_scratch.mkdir(parents=True,exist_ok=True)
fixture=tempfile.TemporaryDirectory(prefix='central-return-native-',dir=native_scratch)
binary=os.environ['CTRL_BIN'];root=pathlib.Path(fixture.name).resolve()/'Central';root.mkdir();checks=[]
def check(v,n):assert v,n;checks.append(n)
def run(op,data):
 p=subprocess.run([binary,'--root',str(root),'--json','action','run',op,json.dumps(data)],capture_output=True,text=True,timeout=15)
 try:return json.loads(p.stdout)
 except Exception:raise RuntimeError((p.returncode,p.stdout,p.stderr))
def good(op,data):
 r=run(op,data);assert r['ok'],r;return r['data']
good('central.init',{});project=root/'Work'/'Proof';project.mkdir();good('projectcentral.init',{'project':'Proof','project_id':'source-return-proof'})
# The return vehicle is an ordinary participating world source (the Project's
# scaffolded working source); the retired Flow registry played no authority.
notes=project/'ProjectCentral/agents/wiki'/'notes.md';notes.parent.mkdir(parents=True, exist_ok=True);notes.write_text('original thought\n')
h=good('projectcentral.change.horizon',{'project':'Proof'})
target=[e for e in h['sources'] if e['binding']['path'].endswith('notes.md')][0]
source=target['binding']['ref'];basis=target['revision']['revision']
path=notes
proposal={'project':'Proof','source_ref':source,'expected_revision':basis,'proposed_content':'returned improvement\n','reason':'real native return','evidence_refs':['evidence:native-test'],'agent_session_ref':'agent-session/native-return'}
r=good('projectcentral.source.return',proposal);ref=r['proposal']['return_ref'];check(r['proposal']['status']=='pending' and path.read_text()=='original thought\n','source return persists proposal without mutating source')
check(r['proposal']['basis_content']=='original thought\n','return preserves exact basis bytes')
check(r['acceptance']['available'],'collaborative return exposes existing native agent authority')
path.write_text('concurrent human writing\n')
conflict=good('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref,'expected_revision':basis,'acceptance':'human-accepted','accepted_by_ref':'human:test'})
check(conflict['outcome']=='conflict' and path.read_text()=='concurrent human writing\n','return acceptance preserves concurrent source and reports conflict')
check(good('projectcentral.source.return_read',{'project':'Proof','return_ref':ref})['proposal']['status']=='pending','conflicted return stays pending')
current=good('projectcentral.source.read',{'project':'Proof','source_ref':source})['revision']['revision']
good('projectcentral.source.write',{'project':'Proof','source_ref':source,'expected_revision':current,'content':'concurrent human writing\n','actor':'test-human','actor_kind':'human'})
current=good('projectcentral.source.read',{'project':'Proof','source_ref':source})['revision']['revision']
r2=good('projectcentral.source.return',dict(proposal,expected_revision=current));ref2=r2['proposal']['return_ref']
accepted=good('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref2,'expected_revision':current,'acceptance':'human-accepted','accepted_by_ref':'human:test'})
check(accepted['outcome']=='accepted' and path.read_text()=='returned improvement\n','explicit collaborative return applies exact proposal through native source CAS')
check(accepted['proposal']['agent_session_ref']=='agent-session/native-return','accepted return retains native agent provenance')
check(not run('projectcentral.source.return_accept',{'project':'Proof','return_ref':ref2,'expected_revision':current,'acceptance':'human-accepted','accepted_by_ref':'human:test'})['ok'],'already accepted return cannot apply twice')
page=good('projectcentral.source.returns',{'project':'Proof','limit':1});check(page['more'] and len(page['entries'])==1 and 'proposed_content' not in page['entries'][0],'return list bounded metadata page')
older=good('projectcentral.source.returns',{'project':'Proof','limit':1,'before':page['next_before']});check(len(older['entries'])==1 and not older['more'],'return list cursor continues without repeats')
rejected=good('projectcentral.source.return_reject',{'project':'Proof','return_ref':ref});check(rejected['proposal']['status']=='rejected' and path.read_text()=='returned improvement\n','rejection leaves source unchanged')
# Several native return candidates compete against the same source revision.
cb=good('projectcentral.source.read',{'project':'Proof','source_ref':source})['revision']['revision']
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
# Native proposal persistence ignores retained legacy staging entries. This
# fresh Project keeps every preceding Return/list/acceptance oracle unchanged.
material=root/'Work/PublicationProof';material.mkdir()
good('projectcentral.init',{'project':'PublicationProof','project_id':'literal-publication-id'})
member=material/'ProjectCentral/agents/wiki/notes.md';member.write_text('native retained source')
h=good('projectcentral.change.horizon',{'project':'PublicationProof'})
selected=next(item for item in h['sources'] if item['binding']['path']=='ProjectCentral/agents/wiki/notes.md')
request={'project':'PublicationProof','source_ref':selected['binding']['ref'],'expected_revision':selected['revision']['revision'],'proposed_content':'native proposal','reason':'actual publisher regression','evidence_refs':[],'agent_session_ref':'agent-session:native-stage-proof'}
area=material/'.central/source-returns';area.mkdir(exist_ok=True)
sentinel=root/'unselected-stage-sentinel';sentinel.write_bytes(b'unselected native fixture bytes')
legacy=area/'record-staging'
for kind in ('hardlink','symlink','fifo','regular'):
 if legacy.exists() or legacy.is_symlink():legacy.unlink()
 if kind=='hardlink':os.link(sentinel,legacy)
 elif kind=='symlink':legacy.symlink_to(sentinel)
 elif kind=='fifo':os.mkfifo(legacy,0o600)
 else:legacy.write_bytes(b'legacy retained record');legacy.chmod(0o644)
 before=legacy.lstat()
 candidate=good('projectcentral.source.return',request)
 after=legacy.lstat()
 check((before.st_dev,before.st_ino,before.st_mode)==(after.st_dev,after.st_ino,after.st_mode),f'actual Return leaves legacy {kind} stage inode/form/mode unchanged')
 check(sentinel.read_bytes()==b'unselected native fixture bytes',f'actual Return never overwrites unselected {kind} sentinel')
 if kind=='regular':check(legacy.read_bytes()==b'legacy retained record','actual Return retains legacy regular stage bytes')
 reference=candidate['proposal']['return_ref']
 record=next(path for path in area.glob('return-*.json') if json.loads(path.read_text())['return_ref']==reference)
 check(json.loads(record.read_text())['source_ref']==request['source_ref'],'published proposal retains exact native SourceRef')
 if kind=='regular':
  attribute=b'org.central.native-record-proof' if sys.platform=='darwin' else b'user.central-native-record-proof'
  if sys.platform=='darwin':
   subprocess.run(['/usr/bin/xattr','-wx',attribute.decode('ascii'),b'native extension metadata'.hex(),str(record)],check=True,timeout=5)
  else:os.setxattr(record,attribute,b'native extension metadata')
  acl_before=None
  if sys.platform=='darwin':
   subprocess.run(['/bin/chmod','+a','everyone allow read',str(record)],check=True,timeout=5)
   acl_before=[line.strip() for line in subprocess.check_output(['/bin/ls','-lde',str(record)],text=True,timeout=5).splitlines()[1:] if line.strip()]
   check(bool(acl_before),'actual native ACL prerequisite is present')
  record.chmod(0o440);before_owner=record.stat()
  rejected=good('projectcentral.source.return_reject',{'project':'PublicationProof','return_ref':reference})
  check(rejected['proposal']['status']=='rejected','owner replaces operational readonly proposal without changing Source authority')
  after_owner=record.stat()
  check((after_owner.st_uid,after_owner.st_gid,stat.S_IMODE(after_owner.st_mode))==(before_owner.st_uid,before_owner.st_gid,0o440),'actual proposal update retains UID/GID/readonly mode')
  if sys.platform=='darwin':
   actual_attribute=bytes.fromhex(subprocess.check_output(['/usr/bin/xattr','-px',attribute.decode('ascii'),str(record)],text=True,timeout=5))
  else:actual_attribute=os.getxattr(record,attribute)
  check(actual_attribute==b'native extension metadata','actual proposal update retains xattr bytes')
  if acl_before is not None:
   acl_after=[line.strip() for line in subprocess.check_output(['/bin/ls','-lde',str(record)],text=True,timeout=5).splitlines()[1:] if line.strip()]
   check(acl_after==acl_before,'actual Mac native proposal ACL survives replacement')
 check(member.read_text()=='native retained source','proposal/rejection never changes selected source bytes')
fixture_outer=pathlib.Path(fixture.name);fixture.cleanup()
check(not fixture_outer.exists(),'actual native fixture removes its owned outer after all Return proofs')
print(json.dumps({'ok':True,'count':len(checks),'checks':checks,'root':str(root),'binary':binary},indent=2))
