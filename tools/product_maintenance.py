#!/usr/bin/env python3
"""Check the real CLI, code evidence and dated documentation maintenance contract."""
from __future__ import annotations
import argparse, csv, datetime, fnmatch, hashlib, io, json, os, re, subprocess
from pathlib import Path
from urllib.parse import urlsplit, unquote
import capability_matrix
import check_product_ground

PRODUCTS = [('Central','central',0),('Actuation','actuation',1),('ai-kit','aikit',2),('Software-Factory','factory',3),('Workcell','workcell',4),('Quaternal-Logic','ql',5)]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(argv, root):
    env = dict(os.environ, NO_COLOR='1', COLUMNS='200')
    result = subprocess.run(argv, cwd=root, env=env, text=True, capture_output=True, timeout=120)
    if result.returncode:
        raise ValueError(f"Discovery failed ({result.returncode}): {argv!r}: {result.stderr[:1000]}")
    return result.stdout

def help_commands(output):
    section = re.search(r'^Commands:\s*\n(.*?)(?=^\S|\Z)', output, re.M | re.S)
    if not section:
        return []
    return [m[1] for m in re.finditer(r'^  ([a-z][a-z0-9-]*)[ \t]{2,}', section[1], re.M) if m[1] != 'help']

def discover(root, config):
    argv = config.get('argv')
    if not isinstance(argv,list) or not argv or any(not isinstance(x,str) or not x for x in argv):
        raise ValueError('maintenance.cli.argv requires an explicit argument array')
    if config.get('format') == 'json':
        items = json.loads(run(argv, root))
        for key in config.get('items_path',[]):
            items = items[key]
        if not isinstance(items,list):
            raise ValueError('CLI discovery selector must resolve to an array')
        field = config.get('id_field')
        commands = [item[field] if field else item for item in items]
    elif config.get('format') in {'clap-help','help'}:
        commands=[]; pending=[[]]; seen=set()
        while pending:
            path=pending.pop(0)
            if tuple(path) in seen or len(seen)>1000:
                raise ValueError('CLI help discovery repeated or exceeded bounded command tree')
            seen.add(tuple(path))
            children=help_commands(run(argv+path+['--help'],root))
            if children and (not path or config['format']=='clap-help'):
                pending.extend(path+[child] for child in children)
            elif path:
                commands.append(' '.join(path))
    else:
        raise ValueError('Unknown CLI discovery format')
    if not commands or any(not isinstance(c,str) or not c.strip() for c in commands):
        raise ValueError('CLI discovery must return nonempty command identities')
    if len(commands)!=len(set(commands)):
        raise ValueError('CLI discovery returned duplicate command identities')
    return sorted(commands)

def check(root, account, namespace, index, *, base=None, reference_scope='workspace', execute=True):
    errors=check_product_ground.validate(root, account, index, namespace, reference_scope=reference_scope)
    manifest,header,records=capability_matrix.load(root/'ProjectCentral/user/capability-matrix.json')
    from reconcile_product_ground import make_plan, record_hash, render_cli_catalog
    markdown=(root/'ProjectCentral/user/capability-matrix.md').read_text()
    if render_cli_catalog(markdown,records)!=markdown:errors.append('Readable CLI catalogue is stale; reconcile it from the capability records')
    plan=make_plan(root,account,namespace,'html-to-csv')
    if plan['seed_changes']:
        errors.append('Seed/CSV drift requires directional reconciliation and expanded-section review: '+','.join(c['row'] for c in plan['seed_changes']))
    if plan['changed_records']:
        errors.append('Matrix changes need account reconciliation: '+','.join(plan['changed_records']))
    parsed=check_product_ground.read_account(root/'ProjectCentral/user'/account)
    provenance=json.loads(parsed.scripts['account-provenance'])
    basis=provenance.get('matrix_basis',{})
    for field,name in [('csv_sha256','capability-matrix.csv'),('manifest_sha256','capability-matrix.json')]:
        if basis.get(field)!=digest(root/'ProjectCentral/user'/name):errors.append('Account matrix basis is stale: '+name)
    maintenance=manifest.get('maintenance',{})
    if not isinstance(maintenance,dict) or not maintenance.get('cli'):
        return errors+['Manifest requires maintenance.cli discovery contract'], {}
    patterns=maintenance.get('runtime_paths')
    if not isinstance(patterns,list) or not patterns or any(not isinstance(p,str) or not p.strip() for p in patterns):
        errors.append('maintenance.runtime_paths requires nonempty path patterns');patterns=[]
    excludes=maintenance.get('runtime_excludes',['**/*.test.*','**/tests/**'])
    if not isinstance(excludes,list) or any(not isinstance(p,str) or not p.strip() for p in excludes):
        errors.append('maintenance.runtime_excludes requires path patterns');excludes=[]
    known=set(); covered={}; code_coverage={}; gaps=[]
    for record in records:
        if record['record_type']!='capability': continue
        ident=record['id']; ext=json.loads(record['extensions']); commands=ext.get('cli_commands'); exposure=ext.get('cli_exposure',{}); receipt=ext.get('maintenance',{})
        if not isinstance(commands,list) or any(not isinstance(c,str) or not c.strip() for c in commands) or len(commands)!=len(set(commands)):
            errors.append(f'{ident}: cli_commands must be a unique string array');commands=[]
        for command in commands: covered.setdefault(command,[]).append(ident)
        if exposure.get('kind') not in {'direct','composed','library','intent','gap'} or not exposure.get('reason','').strip():
            errors.append(f'{ident}: explicit CLI exposure kind and reason required')
        if exposure.get('kind')=='direct' and not commands: errors.append(f'{ident}: direct capability has no CLI command')
        if exposure.get('kind')=='gap':gaps.append(ident)
        try:
            date=datetime.date.fromisoformat(receipt['updated_at'])
            if date>datetime.datetime.now(datetime.timezone.utc).date():raise ValueError('future date')
        except (ValueError,KeyError,TypeError):errors.append(f'{ident}: maintenance.updated_at requires a real non-future ISO date')
        refs=receipt.get('change_refs')
        if not isinstance(refs,list) or not refs or any(not isinstance(r,str) or not r.strip() for r in refs):errors.append(f'{ident}: dated maintenance requires ticket/session/wayfinder change_refs')
        basis=receipt.get('code_basis',{})
        if not isinstance(basis,dict):errors.append(f'{ident}: code_basis must be an object');basis={}
        for ref in filter(None,record['code_refs'].split(';')):
            path=unquote(urlsplit(ref.strip()).path)
            target=(root/path).resolve()
            if not target.is_relative_to(root):
                errors.append(f'{ident}: native code basis must be inside owning repository: {path}');continue
            code_coverage.setdefault(path,[]).append(ident)
            if not target.is_file():continue
            if basis.get(path)!=digest(target):errors.append(f'{ident}: code changed without reconciled capability evidence: {path}')
    if execute:
        try:
            known=set(discover(root,maintenance['cli']))
            for command in sorted(known-set(covered)):errors.append(f'Discoverable CLI command is unmapped: {command}')
            for command in sorted(set(covered)-known):errors.append(f'Matrix names an undiscoverable CLI command: {command}')
        except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError) as exc:errors.append(f'CLI discovery: {exc}')
    if base:
        result=subprocess.run(['git','diff','--name-only',base,'--'],cwd=root,capture_output=True,text=True)
        if result.returncode:errors.append('Cannot resolve comparison base: '+result.stderr.strip())
        else:
            if not patterns:errors.append('maintenance.runtime_paths is required for changed-code coverage')
            for path in result.stdout.splitlines():
                if any(fnmatch.fnmatch(path,pattern) for pattern in patterns) and not any(fnmatch.fnmatch(path,pattern) for pattern in excludes) and (root/path).is_file() and path not in code_coverage:
                    errors.append(f'Changed runtime source has no capability mapping: {path}')
    return errors,{'discovered_commands':len(known),'mapped_commands':len(covered),'capabilities':sum(r['record_type']=='capability' for r in records),'exposure_gaps':gaps,'reference_scope':reference_scope}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,default=Path.cwd());p.add_argument('--account',required=True);p.add_argument('--namespace',required=True);p.add_argument('--product-index',type=int,required=True,choices=range(6));p.add_argument('--base');p.add_argument('--reference-scope',choices=['workspace','repository'],default='workspace');p.add_argument('--discover-only',action='store_true');args=p.parse_args();root=args.root.resolve()
    try:
        if args.discover_only:
            manifest,_,_=capability_matrix.load(root/'ProjectCentral/user/capability-matrix.json');print(json.dumps(discover(root,manifest['maintenance']['cli']),indent=2));return 0
        errors,report=check(root,args.account,args.namespace,args.product_index,base=args.base,reference_scope=args.reference_scope)
    except (OSError,ValueError,KeyError,csv.Error) as exc:errors=[str(exc)];report={}
    print(json.dumps({'errors':errors,**report},indent=2));return int(bool(errors))
if __name__=='__main__':raise SystemExit(main())
