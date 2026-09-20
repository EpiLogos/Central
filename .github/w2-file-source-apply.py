from pathlib import Path
p=Path('ctrl/src/files.rs')
s=p.read_text()
old='''        return crate::source_horizon::control_source_bindings(root)
            .ok()?
            .into_iter()
            .find(|binding| binding.path == relative);'''
new='''        let binding = crate::source_horizon::control_source_bindings(root)
            .ok()?
            .into_iter()
            .find(|binding| binding.path == relative)?;
        // The ratified ordinary Flow-instance carrier already has its own
        // file CAS/history door. An aperture fallback must not reclassify it
        // after its first save and disable that door. Explicit document or
        // governance bindings still win and retain source-owner protection.
        if Path::new(relative).starts_with("Control/user/flows")
            && binding.roles == ["personal-human-source-aperture"]
            && binding.provenance == "unresolved"
            && binding.standing == "unspecified"
            && binding.treatment == "control-user"
        {
            return None;
        }
        return Some(binding);'''
assert s.count(old)==1
p.write_text(s.replace(old,new))
