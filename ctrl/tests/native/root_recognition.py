#!/usr/bin/env python3
"""Recognition through actual ctrl; source-tree snapshots prove no mutation."""
import hashlib,json,os,pathlib,stat,subprocess,tempfile
binary=os.environ['CTRL_BIN'];base=pathlib.Path(tempfile.mkdtemp(prefix='central-recognition-native-')).resolve();checks=[]
def check(v,n):assert v,n;checks.append(n)
def invoke(path):
 p=subprocess.run([binary,'--root',str(base/'never-bind-or-create'),'--json','action','run','central.recognize',json.dumps({'path':str(path)})],capture_output=True,text=True,timeout=15)
 result=json.loads(p.stdout);assert result['ok'],result;return result['data']
def snap(path):
 result={}
 def visit(p):
  m=p.lstat();key=str(p.relative_to(path));entry=[m.st_mode,m.st_ino,m.st_size,m.st_mtime_ns]
  if stat.S_ISREG(m.st_mode):entry.append(hashlib.sha256(p.read_bytes()).hexdigest())
  if stat.S_ISLNK(m.st_mode):entry.append(os.readlink(p))
  result[key]=entry
  if stat.S_ISDIR(m.st_mode):
   for child in p.iterdir():visit(child)
 visit(path);return result
root=base/'Existing';p=subprocess.run([binary,'--root',str(root),'--json','init'],capture_output=True,text=True);assert json.loads(p.stdout)['ok'],p.stdout
(root/'Control/user/note.md').write_text('private sentinel that recognition must not disclose')
(root/'.git').mkdir();os.mkfifo(root/'.git/config')
before=snap(root);r=invoke(root);after=snap(root)
check(r['schema']=='central.root-recognition/v1' and r['outcome']=='recognized','recognizes actual native Central structure')
check(len(r['checks'])==6 and all(c['status']=='present' for c in r['checks']),'recognition is six fixed structural checks')
check(r['canonical_path']==str(root) and r['identity']=={'device':str(root.stat().st_dev),'inode':str(root.stat().st_ino)},'reports exact canonical directory identity')
check(before==after,'recognized source tree remains byte and metadata identical')
check('private sentinel' not in json.dumps(r),'recognition discloses no source content')
check(not (base/'never-bind-or-create').exists() and not r['mutated'] and not r['bound'],'explicit path does not initialize or bind active root')
check((root/'.git/config').is_fifo(),'recognition does not inspect or read git config FIFO')
bare=base/'Unrecognized';bare.mkdir();(bare/'keep.txt').write_text('unadopted');before=snap(bare);u=invoke(bare)
check(u['outcome']=='unrecognized' and snap(bare)==before,'unrecognized directory remains unadopted and unchanged')
check(not (bare/'.central').exists() and not (bare/'ProjectCentral').exists(),'recognition creates no owner state or Project identity')
root.chmod(0o555)
try:
 readonly=invoke(root);check(readonly['outcome']=='recognized' and readonly['access']['read_only'],'recognizes read-only existing ground without probing writes')
finally:root.chmod(0o755)
blocked=base/'Inaccessible';blocked.mkdir();blocked.chmod(0)
try:check(invoke(blocked)['outcome']=='inaccessible','inaccessible chosen directory is distinguished')
finally:blocked.chmod(0o755)
missing=base/'Missing';check(invoke(missing)['outcome']=='missing' and not missing.exists(),'missing directory is reported without initialization')
check(invoke(bare/'keep.txt')['outcome']=='not_directory','regular file cannot become root')
alias=base/'Chosen alias';alias.symlink_to(root,target_is_directory=True);a=invoke(alias)
check(a['outcome']=='recognized' and a['redirected'] and a['canonical_path']==str(root) and a['identity']==r['identity'],'chosen root alias discloses canonical target and exact identity')
state=root/'.central';saved=root/'.central-saved';state.rename(saved);outside=base/'Outside';outside.mkdir();state.symlink_to(outside,target_is_directory=True)
try:
 before=snap(outside);bad=invoke(root);check(bad['outcome']=='unrecognized' and next(c for c in bad['checks'] if c['path']=='.central')['status']=='redirected','redirected required structure is refused')
 check(snap(outside)==before,'redirected member target remains unchanged')
finally:state.unlink();saved.rename(state)
p=subprocess.run([binary,'--json','action','run','central.recognize',json.dumps({'path':'relative'})],capture_output=True,text=True);check(not json.loads(p.stdout)['ok'],'relative path cannot silently resolve against active ground')
print(json.dumps({'ok':True,'count':len(checks),'checks':checks,'root':str(base),'binary':binary},indent=2))
