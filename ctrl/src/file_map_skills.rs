//! Exact owner-authorised skill bundles. Files remain ordinary source; the
//! returned tree is a bounded snapshot, not a second source registry.
use crate::file_map::*;
use crate::projectcentral_flow::content_revision_bytes;
use base64::Engine;
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn members(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let mut stack = vec![(path.to_owned(), 0usize)];
    let mut dirs = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        dirs += 1;
        if dirs > 4096 || depth > 24 {
            return Err(invalid("Skill tree traversal exceeds bounds"));
        }
        if !crate::source_horizon::retrieval_allowed(Path::new("/"), &dir.join("member")) {
            return Err(denied(
                "Skill subtree is excluded by current retrieval policy",
            ));
        }
        for item in fs::read_dir(&dir)? {
            let item = item?;
            let p = item.path();
            let meta = fs::symlink_metadata(&p)?;
            if meta.file_type().is_symlink() {
                return Err(denied("Skill tree contains an unregistered symlink"));
            }
            if meta.is_dir() {
                if matches!(item.file_name().to_str(), Some(".git" | ".central")) {
                    continue;
                }
                stack.push((p, depth + 1));
            } else if meta.is_file() && meta.nlink() == 1 {
                if result.len() == 4096 {
                    return Err(invalid("Skill tree exceeds file-count bound"));
                }
                result.push(p);
            } else {
                return Err(denied(
                    "Skill tree contains special or multiply-linked material",
                ));
            }
        }
    }
    result.sort();
    Ok(result)
}
pub(crate) fn tree(all: &[Scope], input: &Value) -> io::Result<Value> {
    // Resolve applies Project scope and declared cross-scope links as for reads.
    let resolved = resolve(all, input)?;
    let (scope, source) = lookup(all, text(input, "source_ref")?)?;
    if source.kind != "directory" {
        return Err(invalid("Skill source requires a registered directory"));
    }
    let excluded = context_exclusions(all, selected(all, input)?)?;
    let initial = members(&source.path)?;
    let refs: BTreeMap<_, _> = entries(&scope)?
        .into_iter()
        .map(|e| (e.path, e.source.source_ref))
        .collect();
    let mut files = Vec::new();
    let mut skills = Vec::new();
    let mut basis = Vec::new();
    let mut total = 0usize;
    for p in &initial {
        let relative = p
            .strip_prefix(&source.path)
            .map_err(io::Error::other)?
            .to_str()
            .ok_or_else(|| invalid("Non-UTF8 skill path"))?;
        let absolute = p
            .strip_prefix("/")
            .map_err(io::Error::other)?
            .to_str()
            .ok_or_else(|| invalid("Non-UTF8 skill path"))?;
        let f = crate::file_mutation::open_native_file(Path::new("/"), absolute)?;
        let meta = f.metadata()?;
        if !meta.is_file() || meta.nlink() != 1 {
            return Err(denied("Skill file changed its material kind"));
        }
        let mut bytes = Vec::new();
        f.take((crate::source_safety::MAX_SOURCE + 1) as u64)
            .read_to_end(&mut bytes)?;
        total += bytes.len();
        if bytes.len() > crate::source_safety::MAX_SOURCE || total > 32 * 1024 * 1024 {
            return Err(invalid("Skill tree exceeds material-size bound"));
        }
        let revision = content_revision_bytes(&bytes);
        let mode = meta.mode() & 0o777;
        let reference = refs
            .get(p)
            .cloned()
            .unwrap_or_else(|| format!("{}/{}", source.source.source_ref, relative));
        if !policy_allows(&excluded, &reference) {
            return Err(denied(
                "A skill member is excluded by the requesting World's source relations",
            ));
        }
        basis.push(format!("{relative}\0{revision}\0{mode}"));
        files.push(json!({"path":relative,"source_ref":reference,"revision":revision,"mode":mode,"content_base64":base64::engine::general_purpose::STANDARD.encode(&bytes)}));
        if p.file_name().is_some_and(|n| n == "SKILL.md") {
            let dir = p.parent().unwrap();
            let manifest = crate::control_skills::read_skill_manifest(dir)?;
            if let Some(m) = &manifest {
                if dir.file_name().and_then(|s| s.to_str()) != Some(&m.name) {
                    return Err(invalid("Skill manifest name differs from its directory"));
                }
                if m.standing == crate::control_skills::SkillStanding::Retired
                    && m.retirement
                        .as_ref()
                        .is_none_or(|r| r.retirement_reason.trim().is_empty())
                {
                    return Err(invalid("Retired skill lacks retirement provenance"));
                }
            }
            skills
                .push(json!({"path":dir.strip_prefix(&source.path).unwrap(),"manifest":manifest}));
        }
    }
    if members(&source.path)? != initial {
        return Err(conflict("Skill tree membership changed during snapshot"));
    }
    for file in &files {
        let p = source.path.join(text(file, "path")?);
        let rel = p.strip_prefix("/").unwrap().to_str().unwrap();
        safe_member(Path::new("/"), rel, true)?;
        if crate::source_horizon::content_revision(&p)?.revision != text(file, "revision")?
            || fs::metadata(&p)?.mode() & 0o777 != file["mode"].as_u64().unwrap() as u32
        {
            return Err(conflict(
                "Skill bytes or executable mode changed during snapshot",
            ));
        }
    }
    // Recheck source binding and policy after reading the complete tree.
    let now = resolve(all, input)?;
    if now["path"] != resolved["path"] {
        return Err(conflict("Skill source moved during snapshot"));
    }
    let current_exclusions = context_exclusions(all, selected(all, input)?)?;
    if files.iter().any(|file| {
        !policy_allows(
            &current_exclusions,
            file["source_ref"].as_str().unwrap_or_default(),
        )
    }) {
        return Err(denied("Skill member disclosure changed during snapshot"));
    }
    basis.sort();
    Ok(
        json!({"source_ref":source.source.source_ref,"world_ref":scope.world,"project":scope.project,"path":source.path,"tree_revision":content_revision_bytes(basis.join("\n").as_bytes()),"files":files,"skills":skills,"projection_policy":{"standing_owner":"Central","materialisation_owner":"AIKit","loaded_harness_claim":false}}),
    )
}
