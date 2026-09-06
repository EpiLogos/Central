#!/usr/bin/env python3
"""Plan and apply reviewed HTML/CSV reconciliation, with exact bases and recovery."""
from __future__ import annotations
import argparse,csv,datetime,hashlib,html,io,json,os,re,uuid
from pathlib import Path
from urllib.parse import urlsplit
import capability_matrix as matrix
from check_product_ground import read_account

def sha(data):return hashlib.sha256(data).hexdigest()
def normal(text):return ' '.join(text.split())
def record_hash(record):return sha(json.dumps(record,sort_keys=True,ensure_ascii=False).encode())
def files(root,account):
    if Path(account).name!=account or not account.endswith('.html'):raise ValueError('Account must be an HTML basename')
    root=root.resolve();folder=root/'ProjectCentral/user'
    paths={name:folder/name for name in [account,'capability-matrix.csv','capability-matrix.json','capability-matrix.md']}
    if not folder.resolve().is_relative_to(root) or any(not p.resolve().is_relative_to(folder.resolve()) for p in paths.values()):raise ValueError('Source companions must remain inside the selected project ground')
    return paths
def seed_sections(raw,namespace):
    result={}
    for match in re.finditer(r'<section class="ql-node".*?</section>',raw,re.S):
        attrs=dict(re.findall(r'([\w-]+)="([^"]*)"',match[0].split('>',1)[0]))
        key=attrs.get('data-resource','')
        if key.startswith(namespace+':seed:'):
            question=re.search(r'<span data-seed-question="'+re.escape(key)+r'">(.*?)</span>',match[0],re.S)
            if question is None:raise ValueError('Seed requires its visible data-seed-question span: '+key)
            attrs['data-question']=normal(html.unescape(re.sub('<[^>]+>',' ',question[1])))
            result[key.rsplit(':',1)[1]]=(attrs,match[0])
    if set(result)!={f'q{i}' for i in range(6)}:raise ValueError('Expected six unique seed sections')
    return result

def make_plan(root,account,namespace,direction):
    paths=files(root,account);raw=paths[account].read_text();parsed=read_account(paths[account]);sections=seed_sections(raw,namespace)
    manifest,_,records=matrix.load(paths['capability-matrix.json']);view=next(v for v in manifest['views'] if v['id']=='product-field');rowmembers={m['id']:m for m in view['row_axis']['members']}
    provenance=json.loads(parsed.scripts['account-provenance']);old=provenance.get('matrix_basis',{}).get('record_sha256',{});changed=[];required=set();seeds=[]
    for record in records:
        if old.get(record['id'])!=record_hash(record):
            changed.append(record['id'])
            if record['view_id']=='product-field':required.add(record['row_id'])
            if record['record_type']=='capability':
                route=urlsplit(record['account_ref']).fragment.split('/')[0]
                if re.fullmatch('q[0-5]',route):required.add(route)
                for surface,attrs in parsed.nodes:
                    if surface and surface!='whole' and record['id'] in next((seg[0] for seg in re.finditer(r'<section class="ql-node".*?</section>',raw,re.S) if f'id="{attrs["id"]}"' in seg[0]),''):required.add(surface)
    for q,(attrs,segment) in sections.items():
        selected=[r for r in records if r['view_id']=='product-field' and r['row_id']==q and r['column_id']=='S']
        if len(selected)!=1:raise ValueError(f'{q}: one seed-carrying S relation required')
        row=selected[0];htmltext=normal(parsed.seed_text[f'{namespace}:seed:{q}']);csvtext=normal(row['relation']);htmlquestion=attrs['data-question'];csvquestion=row.get('question','')
        if not csvquestion.strip():raise ValueError(f'{q}: CSV question is required')
        text=htmltext if direction=='html-to-csv' else csvtext;question=htmlquestion if direction=='html-to-csv' else csvquestion
        if not text or not question:raise ValueError(f'{q}: empty seed or question')
        stale=parsed.surface_attrs[q].get('data-seed-sha256')!=sha(text.encode())
        if htmltext!=csvtext or htmlquestion!=csvquestion or rowmembers[q]['label']!=csvquestion or stale:
            required.add(q);seeds.append({'row':q,'text':text,'question':question,'html_text':htmltext,'csv_text':csvtext})
    return {'protocol':'product-ground-change/1','root':str(root.resolve()),'account':account,'namespace':namespace,'direction':direction,'basis':{k:sha(p.read_bytes()) for k,p in paths.items()},'changed_records':changed,'seed_changes':seeds,'required_review':sorted(required),'review_meaning':'Read affected expanded sections and linked capabilities; edit substantive prose before creating the final plan. Apply records this reviewed basis; it does not generate or ratify intent.'}

def render_field(md,manifest,records,account,sections):
    view=next(v for v in manifest['views'] if v['id']=='product-field');columns=view['column_axis']['members'];rows=view['row_axis']['members'];caps={r['id']:r for r in records if r['record_type']=='capability'}
    def escape(value):return value.replace('|','\\|').replace('\n',' ')
    out='## Seed × field contribution\n\n[View declarations](capability-matrix.json) · [Editable CSV](capability-matrix.csv). Select a populated cell for its source and capability links. Unassessed cells carry no assertion.\n\n| Seed | '+' | '.join(escape(c['label']) for c in columns)+' |\n| --- | '+' | '.join('---' for _ in columns)+' |\n'
    for row in rows:
        cells=[]
        for col in columns:
            selected=[r for r in records if r['view_id']=='product-field' and r['row_id']==row['id'] and r['column_id']==col['id']];ids={x for r in selected for x in json.loads(r['capability_refs'])};label=f'{len(ids)} capabilities' if ids else 'relation';cells.append(f'[{label}](#field-{row["id"]}-{col["id"]})' if selected else 'Unassessed')
        out+=f'| [{escape(row["label"])}]({account}#whole/{sections[row["id"]][0]["data-route"]}) | '+' | '.join(cells)+' |\n'
    out+='\n<details>\n<summary>Read the field contributions and their capability links</summary>\n\n'
    for row in rows:
        for col in columns:
            selected=[r for r in records if r['view_id']=='product-field' and r['row_id']==row['id'] and r['column_id']==col['id']]
            if not selected:continue
            out+=f'<a id="field-{row["id"]}-{col["id"]}"></a>\n\n### {row["label"]} → {col["label"]}\n\n'
            for r in selected:
                out+=r['relation']+'\n\n'
                out+=' · '.join(f'[{c.rsplit(".",1)[-1].replace("-"," ")}](#{c.replace(".","-")})' for c in json.loads(r['capability_refs']))+'\n\n'
                out+=f'[Source account passage]({r["account_ref"]}) · placement: {r["standing"]}.\n\n'
    out+='</details>'
    start=md.index('## Seed × field contribution');end=md.index('</details>',start)+len('</details>')
    return md[:start]+out+md[end:]

def render_cli_catalog(md,records):
    mapping={}
    for record in records:
        if record['record_type']=='capability':
            for command in json.loads(record['extensions']).get('cli_commands',[]):
                mapping.setdefault(command,[]).append(record['id'])
    table='| CLI identity | Capability |\n| --- | --- |\n'
    for command,capabilities in sorted(mapping.items()):
        links=' · '.join(f'[{c}](#{c.replace(".","-")})' for c in sorted(capabilities))
        table+=f'| `{command}` | {links} |\n'
    block='<!-- cli-catalog:start -->\n'+table+'<!-- cli-catalog:end -->'
    if '<!-- cli-catalog:start -->' in md:
        return re.sub(r'<!-- cli-catalog:start -->.*?<!-- cli-catalog:end -->',lambda _:block,md,flags=re.S)
    heading=re.search(r'^## (?:Native CLI (?:command )?catalog(?:ue)?(?: and parity)?|CLI command catalog)\s*$',md,re.M)
    if not heading:raise ValueError('Readable matrix requires a native CLI catalogue section')
    table_match=re.search(r'^\|.*\n(?:^\|.*\n)+',md[heading.end():],re.M)
    if not table_match:raise ValueError('CLI catalogue requires a table')
    start=heading.end()+table_match.start();end=heading.end()+table_match.end()
    return md[:start]+block+'\n'+md[end:]

def atomic(path,data):
    temp=path.with_name(path.name+'.'+uuid.uuid4().hex+'.tmp')
    try:
        with temp.open('xb') as stream:stream.write(data);stream.flush();os.fsync(stream.fileno())
        os.replace(temp,path)
    finally:
        if temp.exists():temp.unlink()

def apply_plan(plan,reviewed,change_ref):
    if plan.get('protocol')!='product-ground-change/1':raise ValueError('Unknown change plan')
    root=Path(plan['root']).resolve();account=plan['account'];ns=plan['namespace'];paths=files(root,account)
    if set(plan['basis'])!=set(paths):raise ValueError('Plan basis must cover exactly the account companion set')
    before={k:p.read_bytes() for k,p in paths.items()}
    if any(sha(before[k])!=h for k,h in plan['basis'].items()):raise ValueError('Stale plan: source changed; no files written')
    actual=make_plan(root,account,ns,plan['direction'])
    if actual!=plan:raise ValueError('Plan contents changed; regenerate from the actual source')
    if not set(plan['required_review'])<=set(reviewed):raise ValueError('Expanded sections require review: '+','.join(sorted(set(plan['required_review'])-set(reviewed))))
    if not change_ref.strip():raise ValueError('An attributable ticket/session/wayfinder change reference is required')
    raw=before[account].decode();sections=seed_sections(raw,ns);manifest,header,records=matrix.load(paths['capability-matrix.json']);view=next(v for v in manifest['views'] if v['id']=='product-field')
    for change in plan['seed_changes']:
        q=change['row'];attrs,oldsection=sections[q];newsection=oldsection
        if plan['direction']=='csv-to-html':
            pattern=r'(<div data-seed-content="'+re.escape(ns+':seed:'+q)+r'">).*?(</div>)'
            newsection=re.sub(pattern,lambda m:m[1]+'<p>'+html.escape(change['text'])+'</p>'+m[2],newsection,flags=re.S)
            newsection=re.sub(r'(<span data-seed-question="'+re.escape(ns+':seed:'+q)+r'">).*?(</span>)',lambda m:m[1]+html.escape(change['question'])+m[2],newsection,flags=re.S)
            raw=raw.replace(oldsection,newsection,1)
        next(m for m in view['row_axis']['members'] if m['id']==q)['label']=change['question']
        record=next(r for r in records if r['view_id']=='product-field' and r['row_id']==q and r['column_id']=='S');record['relation']=change['text'];record['question']=change['question'];ext=json.loads(record['extensions']);ext.update(seed_sha256=sha(change['text'].encode()),seed_ref=f'{ns}:seed:{q}');ext['reconciled_change_ref']=change_ref;record['extensions']=json.dumps(ext,ensure_ascii=False)
        raw=re.sub(r'(<article\b[^>]*data-surface="'+q+r'"[^>]*data-seed-sha256=")[^"]*(")',lambda m:m[1]+sha(change['text'].encode())+m[2],raw)
        raw=re.sub(r'(<article\b[^>]*data-surface="'+q+r'"[^>]*data-gloss=")[^"]*(")',lambda m:m[1]+html.escape(change['question'],quote=True)+m[2],raw)
        # Keep the visible expanded header aligned with the same question.
        surface=re.search(r'<article\b[^>]*data-surface="'+q+r'".*?</article>',raw,re.S)
        if surface:
            content=surface[0]
            header_end=content.find('</header>')
            if header_end>=0:
                surface_header=content[:header_end]
                surface_header=re.sub(r'(<span class="hero-label">).*?(</span>)',lambda m:m[1]+html.escape(change['question'])+m[2],surface_header,flags=re.S)
                raw=raw[:surface.start()]+surface_header+content[header_end:]+raw[surface.end():]

    reviewed_at=datetime.datetime.now(datetime.timezone.utc).isoformat()
    changed_ids=set(plan['changed_records']) | {r['id'] for r in records if r['view_id']=='product-field' and r['column_id']=='S' and r['row_id'] in {c['row'] for c in plan['seed_changes']}}
    for record in records:
        if record['id'] in changed_ids:
            ext=json.loads(record['extensions']);receipt=ext.setdefault('maintenance',{})
            receipt['updated_at']=reviewed_at[:10]
            receipt['change_refs']=list(dict.fromkeys(receipt.get('change_refs',[])+[change_ref]))
            ext['last_reconciled_at']=reviewed_at;record['extensions']=json.dumps(ext,ensure_ascii=False)
    errors=matrix.validate_data(manifest,header,records)
    if errors:raise ValueError('; '.join(errors))
    output=io.StringIO(newline='');writer=csv.DictWriter(output,fieldnames=header,lineterminator='\n');writer.writeheader();writer.writerows(records);csvtext=output.getvalue();manifesttext=json.dumps(manifest,ensure_ascii=False,indent=2)+'\n'
    sections=seed_sections(raw,ns);md=matrix.sync_csv_appendix(before['capability-matrix.md'].decode(),csvtext);md=render_field(md,manifest,records,account,sections);md=render_cli_catalog(md,records)
    parsed=re.search(r'(<script[^>]*id="account-provenance"[^>]*>)(.*?)(</script>)',raw,re.S);provenance=json.loads(parsed[2]);provenance['matrix_basis']={'protocol':matrix.PROTOCOL,'csv_sha256':sha(csvtext.encode()),'manifest_sha256':sha(manifesttext.encode()),'record_sha256':{r['id']:record_hash(r) for r in records},'change_ref':change_ref,'reviewed_at':reviewed_at,'reviewed_sections':sorted(set(reviewed))};raw=raw[:parsed.start(2)]+json.dumps(provenance,ensure_ascii=False,indent=2)+raw[parsed.end(2):]
    after={account:raw.encode(),'capability-matrix.csv':csvtext.encode(),'capability-matrix.json':manifesttext.encode(),'capability-matrix.md':md.encode()}
    journal=root/'.central/documentation-transactions'/uuid.uuid4().hex;journal.mkdir(parents=True,mode=0o700)
    for version,data in [('before',before),('after',after)]:
        (journal/version).mkdir(mode=0o700)
        for name,value in data.items():(journal/version/name).write_bytes(value)
    receipt={'protocol':'product-ground-transaction/1','root':str(root),'account':account,'status':'prepared','before':{k:sha(v) for k,v in before.items()},'after':{k:sha(v) for k,v in after.items()},'change_ref':change_ref,'reviewed_at':reviewed_at,'reviewed_sections':sorted(set(reviewed))}
    def save():atomic(journal/'receipt.json',(json.dumps(receipt,indent=2)+'\n').encode())
    save()
    try:
        for name,path in paths.items():
            if sha(path.read_bytes())!=receipt['before'][name]:raise ValueError('Concurrent edit during apply: '+name)
            atomic(path,after[name])
        receipt['status']='applied';save()
    except BaseException:
        receipt['status']='incomplete';save();raise
    return journal/'receipt.json'

def restore(receipt_path):
    receipt_path=receipt_path.resolve();receipt=json.loads(receipt_path.read_text());root=Path(receipt['root']).resolve();paths=files(root,receipt['account']);journal=receipt_path.parent
    expected=root/'.central/documentation-transactions'
    if journal.parent!=expected.resolve() or receipt_path.name!='receipt.json':raise ValueError('Receipt must belong to this project documentation transaction journal')
    if receipt.get('status') not in {'prepared','applied','incomplete','restored'}:raise ValueError('Invalid transaction status')
    if receipt.get('protocol')!='product-ground-transaction/1' or set(paths)!=set(receipt['before']):raise ValueError('Invalid transaction receipt')
    for name,path in paths.items():
        current=sha(path.read_bytes())
        if current not in {receipt['before'][name],receipt['after'][name]}:raise ValueError('Restore would overwrite later work: '+name)
        if sha((journal/'before'/name).read_bytes())!=receipt['before'][name]:raise ValueError('Recovery bytes do not match receipt')
    for name,path in paths.items():atomic(path,(journal/'before'/name).read_bytes())
    receipt['status']='restored';atomic(receipt_path,(json.dumps(receipt,indent=2)+'\n').encode())

def main():
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='command',required=True)
    plan=sub.add_parser('plan');plan.add_argument('--root',type=Path,required=True);plan.add_argument('--account',required=True);plan.add_argument('--namespace',required=True);plan.add_argument('--direction',choices=['html-to-csv','csv-to-html'],required=True);plan.add_argument('--out',type=Path,required=True)
    apply=sub.add_parser('apply');apply.add_argument('plan',type=Path);apply.add_argument('--reviewed',nargs='*',default=[]);apply.add_argument('--change-ref',required=True)
    undo=sub.add_parser('restore');undo.add_argument('receipt',type=Path)
    args=p.parse_args()
    try:
        if args.command=='plan':result=make_plan(args.root.resolve(),args.account,args.namespace,args.direction);args.out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
        elif args.command=='apply':print(apply_plan(json.loads(args.plan.read_text()),args.reviewed,args.change_ref))
        else:restore(args.receipt);print('Restored exact pre-change companions')
    except (OSError,ValueError,KeyError,csv.Error) as exc:p.exit(1,str(exc)+'\n')
if __name__=='__main__':main()
