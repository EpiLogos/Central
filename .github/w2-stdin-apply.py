from pathlib import Path
p=Path('ctrl/src/cli.rs')
s=p.read_text()
old='''    let value: Value = serde_json::from_str(raw).map_err(|error| {'''
assert s.count(old)==1
s=s.replace(old,'''    // Explicit '-' carries the same JSON-object input on stdin. Large source
    // and original-form payloads must not depend on OS argv limits. This is
    // only transport: registry, authority and per-operation bounds still apply.
    let from_stdin;
    let raw = if raw == "-" {
        use std::io::Read;
        const MAX_ACTION_INPUT: u64 = 16 * 1024 * 1024;
        let mut bytes = Vec::new();
        std::io::stdin().take(MAX_ACTION_INPUT + 1).read_to_end(&mut bytes)
            .map_err(|error| (structured, format!("read action JSON from stdin: {error}")))?;
        if bytes.len() as u64 > MAX_ACTION_INPUT {
            return Err((structured, "action stdin JSON exceeds the 16 MiB input bound".to_owned()));
        }
        from_stdin = String::from_utf8(bytes)
            .map_err(|_| (structured, "action stdin JSON must be UTF-8".to_owned()))?;
        from_stdin.as_str()
    } else {
        raw
    };
    let value: Value = serde_json::from_str(raw).map_err(|error| {''')
p.write_text(s)
Path('ctrl/tests/action_stdin.rs').write_text(r'''//! The real CLI, not a parser mock. Disposable native root only.
use serde_json::{json, Value};
use std::{fs, io::Write, path::{Path,PathBuf}, process::{Command,Stdio}, time::{SystemTime,UNIX_EPOCH}};
struct Ground(PathBuf);
impl Drop for Ground {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn run(root:&Path,action:&str,input:&[u8])->(bool,Value) {
 let mut child=Command::new(env!("CARGO_BIN_EXE_ctrl")).args(["--json","--root"]).arg(root).args(["action","run",action,"-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
 let write=child.stdin.take().unwrap().write_all(input);
 let output=child.wait_with_output().unwrap();
 let value=serde_json::from_slice(&output.stdout).unwrap_or_else(|_|panic!("{}",String::from_utf8_lossy(&output.stderr)));
 if output.status.success(){write.unwrap();}
 (output.status.success(),value)
}
fn ground()->Ground {
 let root=Ground(std::env::temp_dir().join(format!("central-stdin-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())));
 central_ctrl::initialize_central(&root.0).unwrap();root
}
#[test]
fn large_native_root_source_round_trips_without_argv_or_temporary_input_files(){
 let root=ground();fs::write(root.0.join("Control/user/large.md"),"initial").unwrap();
 let (ok,horizon)=run(&root.0,"projectcentral.change.horizon",b"{}");assert!(ok,"{horizon}");
 let source=horizon["data"]["sources"].as_array().unwrap().iter().find(|s|s["binding"]["path"]=="Control/user/large.md").unwrap();
 let text="original form-sized content — ".repeat(20000);
 let request=json!({"source_ref":source["binding"]["ref"],"expected_revision":source["revision"]["revision"],"content":text,"actor":"human:stdin-test","actor_kind":"human"});
 let (ok,written)=run(&root.0,"projectcentral.source.write",&serde_json::to_vec(&request).unwrap());assert!(ok,"{written}");
 let (ok,reopened)=run(&root.0,"projectcentral.source.read",&serde_json::to_vec(&json!({"source_ref":source["binding"]["ref"]})).unwrap());assert!(ok,"{reopened}");
 assert_eq!(reopened["data"]["content"],text);assert_eq!(fs::read_to_string(root.0.join("Control/user/large.md")).unwrap(),text);
}
#[test]
fn invalid_and_oversized_stdin_are_refused_before_any_action(){
 let root=ground();
 for bytes in [b"[]".to_vec(),b"{broken".to_vec(),vec![b' ';16*1024*1024+1]] {
  let (ok,value)=run(&root.0,"projectcentral.source.write",&bytes);assert!(!ok,"{value}");assert_eq!(value["ok"],false);
 }
 assert!(!root.0.join("Control/user/large.md").exists());
}
''')
