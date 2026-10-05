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
# Native commissioned maintenance uses the existing recognized authority Source,
# never an actor label, repository merge or mocked response. These six carriers
# are controlled test material, not the owner's pending six Main afterimages.
import hashlib,time
check(hr['schema']=='central.source-return-reading/v1' and hr['proposal']['schema']=='central.source-return/v1' and 'basis_source' not in hr['proposal'] and 'maintenance_authorization' not in hr['proposal'],'default collaborative v1 wire has no maintenance fields')
cutover={'standing':'NOT_SELECTED_NATIVE_PREREQUISITE_REQUIRED','executed_total':0,'pass_credit':0}
collaborative_current=good('projectcentral.source.read',{'project':'Proof','source_ref':source})
selected_collaborative=good('projectcentral.source.return',dict(proposal,expected_revision=collaborative_current['revision']['revision'],acceptance='commissioned-maintenance'))
check(selected_collaborative['proposal']['schema']=='central.source-return/v2' and not selected_collaborative['acceptance']['available'],'selected v2 cannot advertise default collaborative acceptance as authority')
maintenance_action='projectcentral.source.return_accept'
maintenance_token='source-return-native-maintenance-fixture-token-0001'
maintenance_principal='human:source-return-native-fixture'
authority_path=root/'Control/user/source-return-authority.json'
relations_path=root/'Control/relations/source-relations.json'
relations_path.parent.mkdir(parents=True,exist_ok=True)
original_root_relations=json.loads(relations_path.read_text()) if relations_path.exists() else {'schema':'central.control.ground-relations/v1','project_id':'control:root','relations':[]}
authority_ref='central:source:control:root:Control/user/source-return-authority.json'
authority_relation={'ref':authority_ref,'path':'Control/user/source-return-authority.json','roles':['native-action-authority'],'provenance':'human-adopted','standing':'architecture-contract','treatment':'projectcentral-user','recognition':'controlled-native-fixture-not-personal-adoption','recorded_at_unix_seconds':1}
projects=[]
for selector,identity in [('MaintenanceAIKit','maintenance-ai-kit'),('MaintenanceActuation','maintenance-actuation')]:
    (root/'Work'/selector).mkdir()
    good('projectcentral.init',{'project':selector,'project_id':identity})
    projects.append((selector,'project:'+identity,root/'Work'/selector))
maintenance_scopes=[world for _,world,_ in projects]
def write_authority(actions=None,scopes=None,kind='human',expiry=None):
    grant={'principal_ref':maintenance_principal,'actor_kind':kind,'token_sha256':hashlib.sha256(maintenance_token.encode()).hexdigest(),'scope_refs':maintenance_scopes if scopes is None else scopes,'actions':[maintenance_action] if actions is None else actions,'expires_at_unix_seconds':int(time.time())+600 if expiry is None else expiry}
    authority_path.write_text(json.dumps({'schema':'central.native-action-authority/v1','scope_ref':'control:root','grants':[grant]}))
    relation_document=dict(original_root_relations)
    relation_document['relations']=[r for r in original_root_relations['relations'] if 'native-action-authority' not in r.get('roles',[])]+[authority_relation]
    relations_path.write_text(json.dumps(relation_document))
write_authority()
def maintenance_call(data,token=maintenance_token):
    env=dict(os.environ);env.pop('CENTRAL_NATIVE_TOKEN',None)
    if token is not None:env['CENTRAL_NATIVE_TOKEN']=token
    p=subprocess.run([binary,'--root',str(root),'--json','action','run',maintenance_action,json.dumps(data)],capture_output=True,text=True,timeout=15,env=env)
    try:return json.loads(p.stdout)
    except Exception:raise RuntimeError((p.returncode,p.stdout,p.stderr))
def maintenance_basis(selector,reference):
    value=good('projectcentral.source.return_read',{'project':selector,'return_ref':reference,'acceptance':'commissioned-maintenance'})
    basis=value['commissioned_maintenance']
    check(not basis['available'] and basis['requires_current_authenticated_principal'],'native authority basis is not an unauthenticated grant')
    return basis['authority_revision']
def maintenance_request(selector,candidate):
    proposal=candidate['proposal']
    return {'project':selector,'return_ref':proposal['return_ref'],'expected_revision':proposal['basis_revision'],'acceptance':'commissioned-maintenance','accepted_by_ref':maintenance_principal,'expected_authority_revision':maintenance_basis(selector,proposal['return_ref'])}
def maintenance_proposal(selector,member,new):
    reading=good('projectcentral.source.read',{'project':selector,'source_ref':member['ref']})
    return good('projectcentral.source.return',{'project':selector,'source_ref':reading['source']['ref'],'expected_revision':reading['revision']['revision'],'proposed_content':new,'acceptance':'commissioned-maintenance','reason':'controlled native link-maintenance test','evidence_refs':['evidence:controlled-native-source-maintenance'],'agent_session_ref':'agent-session:maintenance-fixture'})
carriers=[]
for selector,world,owner in projects:
    area=owner/'ProjectCentral/user/telos';area.mkdir(parents=True,exist_ok=True)
    for name,before,after in [('capability-matrix.csv','id,source,status\nkept,old,unknown\n','id,source,status\nkept,new,unknown\n'),('capability-matrix.md','# Authored fixture\n[old](old)\n','# Authored fixture\n[new](new)\n'),('position.html','<article data-id="kept"><a href="old">Authored</a></article>','<article data-id="kept"><a href="new">Authored</a></article>')]:
        path=area/name;path.write_text(before)
        horizon=good('projectcentral.change.horizon',{'project':selector})
        entry=next(e for e in horizon['sources'] if e['binding']['path']=='ProjectCentral/user/telos/'+name)
        member=entry['binding'];candidate=maintenance_proposal(selector,member,after)
        check(candidate['proposal']['schema']=='central.source-return/v2' and candidate['schema']=='central.source-return-reading/v2' and candidate['proposal']['basis_source']==member and path.read_bytes()==before.encode(),'native proposal captures exact binding/basis without source mutation')
        carriers.append((selector,world,owner,path,member,before,after,candidate))
check(len(carriers)==6,'six native CSV/MD/HTML carriers across two actual Projects selected')
selector,world,owner,path,member,before,after,candidate=carriers[0]
# This optional selection is an actual TWO-IMAGE qualification. The gate must
# first qualify the previous binary/Source/lock association; a filesystem path
# alone is never evidence that an executable is the prior native owner. Absent
# selection earns zero downgrade credit and is reported explicitly.
if os.environ.get('SOURCE_RETURN_REQUIRE_CUTOVER')=='1':
    previous_binary=os.environ['SOURCE_RETURN_PREVIOUS_CTRL_BIN']
    assert pathlib.Path(previous_binary).is_absolute() and pathlib.Path(previous_binary).is_file(),'qualified previous native executable is required, no green absence'
    current_record=next(p for p in (owner/'.central/source-returns').glob('return-*.json') if json.loads(p.read_text())['return_ref']==candidate['proposal']['return_ref'])
    original_record=current_record.read_bytes();original_source=path.read_bytes()
    for operation in ('return_read','return_reject','return_accept'):
        data={'project':selector,'return_ref':candidate['proposal']['return_ref']}
        if operation=='return_accept':data.update(expected_revision=candidate['proposal']['basis_revision'],acceptance='human-accepted',accepted_by_ref='human:old-owner-characterization')
        previous_env=dict(os.environ);previous_env.pop('CENTRAL_NATIVE_TOKEN',None)
        observed=subprocess.run([previous_binary,'--root',str(root),'--json','action','run','projectcentral.source.'+operation,json.dumps(data)],capture_output=True,text=True,timeout=15,env=previous_env)
        try:old_reply=json.loads(observed.stdout)
        except Exception:raise RuntimeError((observed.returncode,observed.stdout,observed.stderr))
        check(observed.returncode!=0 and not old_reply['ok'] and old_reply['status']=='invalid_input','previous actual native owner refuses v2 '+operation+' before update')
        check(current_record.read_bytes()==original_record and path.read_bytes()==original_source,'previous actual native '+operation+' preserves exact v2 record/captured facts and source bytes')
    cutover={'standing':'EXECUTED_THREE_NATIVE_REFUSALS_SOURCE_ASSOCIATION_REQUIRED','executed_total':3,'pass_credit':3,'previous_binary':previous_binary}
# Genuine malformed-retained-state characterization: create through the native
# owner, then alter only its operational fixture. No native crash is inferred.
malformed=maintenance_proposal(selector,member,'malformed-state characterization')
malformed_path=next(p for p in (owner/'.central/source-returns').glob('return-*.json') if json.loads(p.read_text())['return_ref']==malformed['proposal']['return_ref'])
malformed_bytes=malformed_path.read_bytes();malformed_value=json.loads(malformed_bytes)
malformed_value['basis_source']['ref']='not-the-captured-source';malformed_path.write_text(json.dumps(malformed_value))
wrong_identity=malformed_path.read_bytes()
check(not run('projectcentral.source.return_reject',{'project':selector,'return_ref':malformed['proposal']['return_ref']})['ok'] and malformed_path.read_bytes()==wrong_identity and path.read_bytes()==before.encode(),'inconsistent v2 captured Source identity refuses before rejection/save')
malformed_value=json.loads(malformed_bytes);malformed_value['status']='applying';malformed_path.write_text(json.dumps(malformed_value))
missing_authorization=malformed_path.read_bytes()
check(not run('projectcentral.source.return_read',{'project':selector,'return_ref':malformed['proposal']['return_ref']})['ok'] and malformed_path.read_bytes()==missing_authorization and path.read_bytes()==before.encode(),'v2 applying without observed authorization refuses before recovery/save')
malformed_path.write_bytes(malformed_bytes)
request=maintenance_request(selector,candidate)
missing=maintenance_call(request,None)
check(not missing['ok'] and missing['status']=='unavailable_capability' and path.read_bytes()==before.encode(),'missing host credential cannot accept protected proposal')
for changed in [dict(request,accepted_by_ref='human:claimed'),dict(request,actor_kind='human'),dict(request,principal={'actor_kind':'human'}),dict(request,token=maintenance_token),dict(request,expected_authority_revision='stale')]:
    check(not maintenance_call(changed)['ok'] and path.read_bytes()==before.encode(),'payload principal/actor/identity or stale authority cannot grant protected mutation')
for kwargs in [dict(actions=['central.receiving.include']),dict(scopes=[maintenance_scopes[1]]),dict(kind='agent'),dict(expiry=0)]:
    write_authority(**kwargs)
    invalid_request=dict(request,expected_authority_revision=maintenance_basis(selector,candidate['proposal']['return_ref']))
    refused=maintenance_call(invalid_request)
    check(not refused['ok'] and refused['status']=='unavailable_capability' and path.read_bytes()==before.encode(),'wrong exact action/Project/Agent principal/expired grant preserves original native source')
write_authority();request=maintenance_request(selector,candidate)
old_authority=request['expected_authority_revision'];write_authority(expiry=int(time.time())+900)
check(not maintenance_call(dict(request,expected_authority_revision=old_authority))['ok'],'changed complete authority Source invalidates selected authority revision')
write_authority()
# A real current-binding change is independent of unchanged content bytes.
project_relations=owner/'ProjectCentral/relations/source-relations.json'
previous_relations=project_relations.read_bytes() if project_relations.exists() else None
relation={'ref':member['ref'],'path':member['path'],'roles':member['roles']+['maintenance-fixture-role'],'provenance':member['provenance'],'standing':member['standing'],'treatment':member['treatment'],'recognition':'controlled-fixture-metadata-change','recorded_at_unix_seconds':1}
project_relations.parent.mkdir(parents=True,exist_ok=True)
project_relations.write_text(json.dumps({'schema':'central.project.ground-relations/v1','project_id':world.removeprefix('project:'),'relations':[relation]}))
changed=maintenance_call(maintenance_request(selector,candidate))
check(changed['ok'] and changed['data']['outcome']=='conflict' and path.read_bytes()==before.encode(),'changed native Source binding cannot accept unchanged copied basis')
if previous_relations is None:project_relations.unlink()
else:project_relations.write_bytes(previous_relations)
# Actual byte change is reported as conflict; the original proposal never rehomes.
path.write_bytes(before.encode()+b'actual external edit\n')
changed=maintenance_call(maintenance_request(selector,candidate))
check(changed['ok'] and changed['data']['outcome']=='conflict' and path.read_bytes()==before.encode()+b'actual external edit\n','current Source byte/revision change remains intact')
path.write_bytes(before.encode())
wrong_project=dict(maintenance_request(selector,candidate),project=projects[1][0])
check(not maintenance_call(wrong_project)['ok'],'ReturnRef from another current Project cannot rebind')
# Actual withdrawal and current retrieval treatment defeat copied proposals.
withdrawal_request=maintenance_request(selector,candidate)
path.unlink()
withdrawn=maintenance_call(withdrawal_request)
check(not withdrawn['ok'] and not path.exists(),'actual native Source withdrawal cannot use retained proposal bytes as current authority')
path.write_bytes(before.encode())
marker_request=maintenance_request(selector,candidate)
marker=path.parent/'.no-agent-retrieval';marker.write_text('controlled native refusal')
try:
    withheld=maintenance_call(marker_request)
    check(not withheld['ok'] and withheld['status']=='unavailable_capability' and path.read_bytes()==before.encode(),'current native retrieval denial refuses before source payload application')
    check(json.dumps(before) not in json.dumps(withheld),'native retrieval refusal emits no complete private fixture body')
finally:marker.unlink()
# The native owner refuses real readonly form and genuine nonroot parent EACCES;
# applying intent is retained/reconciled, no rollback or OS errno is fabricated.
assert os.geteuid()!=0,'real parent EACCES prerequisite must not become a green skip'
path.chmod(0o444)
failed=maintenance_call(maintenance_request(selector,candidate))
check(not failed['ok'] and path.read_bytes()==before.encode(),'actual readonly source failure preserves proposal/basis')
path.chmod(0o644)
old_mode=stat.S_IMODE(path.parent.stat().st_mode);path.parent.chmod(0o555)
try:
    failed=maintenance_call(maintenance_request(selector,candidate))
    check(not failed['ok'] and path.read_bytes()==before.encode(),'real parent publication IO refusal never becomes acceptance or source retry')
finally:path.parent.chmod(old_mode)
failed_read=good('projectcentral.source.return_read',{'project':selector,'return_ref':candidate['proposal']['return_ref']})
check(failed_read['proposal']['status']=='pending' and failed_read['current']['content']==before,'new process observes pending failed native attempt and exact unchanged basis')
legacy_switch=run('projectcentral.source.return_accept',{'project':selector,'return_ref':candidate['proposal']['return_ref'],'expected_revision':candidate['proposal']['basis_revision'],'acceptance':'human-accepted','accepted_by_ref':'human:claimed'})
check(not legacy_switch['ok'] and path.read_bytes()==before.encode(),'retained maintenance intent cannot be relabeled as declared collaborative acceptance')
# Same authentication admits only ordinary human-source apertures, never the
# dedicated owner state. A real native binding carries the denied role.
protected=owner/'ProjectCentral/user/owned.json';protected.write_text('{"schema":"central.document/v1","kind":"flow","document_id":"fixture-native-document","entries":[]}')
protected_ref=next(e['binding'] for e in good('projectcentral.change.horizon',{'project':selector})['sources'] if e['binding']['path']=='ProjectCentral/user/owned.json')
protected_relation=dict(relation,ref=protected_ref['ref'],path=protected_ref['path'],roles=['project-human-source-aperture','protected-contribution-document'])
project_relations.write_text(json.dumps({'schema':'central.project.ground-relations/v1','project_id':world.removeprefix('project:'),'relations':[protected_relation]}))
protected_binding=good('projectcentral.source.read',{'project':selector,'source_ref':protected_ref['ref']})['source']
protected_candidate=maintenance_proposal(selector,protected_binding,'not applied')
check(not maintenance_call(maintenance_request(selector,protected_candidate))['ok'] and 'fixture-native-document' in protected.read_text(),'authenticated maintenance cannot bypass dedicated native document owner')
if previous_relations is None:project_relations.unlink()
else:project_relations.write_bytes(previous_relations)
accepted_records=[]
for selector,world,owner,path,member,before,after,candidate in carriers:
    request=maintenance_request(selector,candidate)
    result=maintenance_call(request);assert result['ok'],result
    accepted=result['data'];receipt=accepted['receipt'];actual=good('projectcentral.source.read',{'project':selector,'source_ref':member['ref']})
    check(accepted['outcome']=='accepted' and path.read_bytes()==after.encode() and actual['content'].encode()==after.encode(),'native acceptance and fresh Source read prove exact complete carrier bytes')
    check(actual['source']==member and receipt['source']['source']==member,'native acceptance preserves complete Source metadata/standing/treatment')
    check(receipt['source']['actor_kind']=='agent' and receipt['source']['agent_session_ref']=='agent-session:maintenance-fixture','native writer keeps Agent execution separate from authorizing principal')
    authorization=receipt['maintenance_authorization']
    check(authorization['principal_ref']==maintenance_principal and authorization['scope_ref']==world and authorization['actor_kind']=='human' and authorization['authority_revision']==request['expected_authority_revision'],'receipt retains actual authenticated origin and exact native authority basis')
    check('token' not in authorization and 'token_sha256' not in authorization and 'permitted_actions' not in authorization,'receipt exports no credential or full grant')
    loaded=good('projectcentral.source.return_read',{'project':selector,'return_ref':candidate['proposal']['return_ref']})
    check(loaded['proposal']['maintenance_authorization']==authorization and loaded['proposal']['agent_session_ref']=='agent-session:maintenance-fixture','restart read retains historical attribution without authorizing another write')
    check(not maintenance_call(request)['ok'],'already completed native proposal cannot apply again')
    accepted_records.append((selector,owner,path,member,after,candidate,authorization))
# Explicit retained-state characterization: rewind only the status of an actual
# native accepted record to its recorded applying intent. This tests recovery
# classification, not an asserted process crash or fabricated native receipt.
selector,owner,path,member,after,candidate,authorization=accepted_records[0]
record_path=next(p for p in (owner/'.central/source-returns').glob('return-*.json') if json.loads(p.read_text())['return_ref']==candidate['proposal']['return_ref'])
record=json.loads(record_path.read_text());record['status']='applying';record['result_revision']=None;record_path.write_text(json.dumps(record))
recovered=good('projectcentral.source.return_read',{'project':selector,'return_ref':record['return_ref']})
check(recovered['proposal']['status']=='accepted' and recovered['proposal']['maintenance_authorization']==authorization and path.read_bytes()==after.encode(),'actual target recovery retains native origin/Agent facts without source resend')
third=maintenance_proposal(selector,member,after+'third native revision\n')
third_result=maintenance_call(maintenance_request(selector,third));assert third_result['ok'],third_result
record['status']='applying';record_path.write_text(json.dumps(record))
third_read=run('projectcentral.source.return_read',{'project':selector,'return_ref':record['return_ref']})
check(not third_read['ok'] and path.read_bytes()==(after+'third native revision\n').encode(),'third actual native revision remains unresolved with no automatic resend')
# A legacy record is readable but cannot be upgraded by guessing a missing binding.
legacy_candidate=maintenance_proposal(selector,member,'legacy proposed source')
legacy_path=next(p for p in (owner/'.central/source-returns').glob('return-*.json') if json.loads(p.read_text())['return_ref']==legacy_candidate['proposal']['return_ref'])
legacy=json.loads(legacy_path.read_text());legacy['schema']='central.source-return/v1';legacy.pop('basis_source');legacy_path.write_text(json.dumps(legacy))
check(good('projectcentral.source.return_read',{'project':selector,'return_ref':legacy['return_ref']})['proposal']['status']=='pending','legacy SourceReturn remains readable')
check(not maintenance_call(maintenance_request(selector,legacy_candidate))['ok'],'legacy missing binding requires a fresh exact proposal for maintenance')

fixture_outer=pathlib.Path(fixture.name);fixture.cleanup()
check(not fixture_outer.exists(),'actual native fixture removes its owned outer after all Return proofs')
print(json.dumps({'ok':True,'count':len(checks),'checks':checks,'root':str(root),'binary':binary,'previous_owner_cutover':cutover},indent=2))
