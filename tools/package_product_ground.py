#!/usr/bin/env python3
"""Build/check the reproducible CI tool bundle maintained by Central.

The bundle lets each product validate its own checkout without cross-repository
credentials or a mutable download of another product's code during CI.
"""
import argparse,hashlib,io,json,os,tempfile,zipfile
from pathlib import Path

SOURCES=['capability_matrix.py','check_product_ground.py','product_maintenance.py','reconcile_product_ground.py']

def bundle():
    folder=Path(__file__).resolve().parent
    payload={name:(folder/name).read_bytes() for name in SOURCES}
    manifest={'protocol':'product-ground-tools/1','owner':'EpiLogos/Central','sources':{k:hashlib.sha256(v).hexdigest() for k,v in payload.items()}}
    payload['bundle.json']=(json.dumps(manifest,sort_keys=True,indent=2)+'\n').encode()
    payload['__main__.py']=b'from product_maintenance import main\nraise SystemExit(main())\n'
    output=io.BytesIO()
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for name,data in sorted(payload.items()):
            info=zipfile.ZipInfo(name,date_time=(2026,1,1,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o644<<16;archive.writestr(info,data)
    return output.getvalue()

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,action='append');p.add_argument('--check',action='store_true');a=p.parse_args();roots=a.root or [Path(__file__).resolve().parents[1]];data=bundle();failed=False
    for root in roots:
        target=root/'.github/product-ground.pyz'
        if a.check:
            if not target.is_file() or target.read_bytes()!=data:print('Stale CI tool bundle: '+str(target));failed=True
        else:
            target.parent.mkdir(parents=True,exist_ok=True)
            temporary=None
            try:
                with tempfile.NamedTemporaryFile(dir=target.parent,delete=False) as stream:
                    temporary=Path(stream.name);stream.write(data);stream.flush();os.fsync(stream.fileno())
                os.replace(temporary,target);print(target)
            finally:
                if temporary is not None and temporary.exists():temporary.unlink()
    return int(failed)
if __name__=='__main__':raise SystemExit(main())
