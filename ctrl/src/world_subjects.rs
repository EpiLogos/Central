//! `central.world.subjects`: the bounded entities a world holds, as a Being or a Thing page names its Subject (manual §18.1
//! "A being or a subject in the world"). Central owns the pasu grammar (`central.pasu/v1`: nara, agent, agent-set) and the
//! records each subject stands on — the person's identity manifest in your world, and each register's agent profiles and agent
//! sets — so the listing is Central's, read from those records and never stored. A subject is named by its pasu ref; the record
//! it stands on is named beside it. Reading changes nothing.
use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionOutputDefinition,
    ActionRegistry, MutationClass,
};
use crate::agent_set_store::RelationRecordStore;
use crate::pasu::{PasuIdentityManifest, PasuRef, PASU_IDENTITY_MANIFEST_PATH};
use crate::personal::string_input;
use crate::result::{ActionResult, ResultStatus};
use serde_json::{json, Value};

pub const WORLD_SUBJECTS_ACTION: &str = "central.world.subjects";
pub const WORLD_SUBJECTS_SCHEMA: &str = "central.world-subjects/v1";

fn failure(message: impl Into<String>) -> ActionResult {
    ActionResult::failure(
        Some(WORLD_SUBJECTS_ACTION),
        ResultStatus::InvalidInput,
        message,
        None,
    )
}

fn world_subjects(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    // The register is resolved exactly as the agent profiles' own actions resolve it: `root` (or `personal`), or one project.
    let profiles =
        match crate::agent_profile_actions::resolve_store(WORLD_SUBJECTS_ACTION, input, context) {
            Ok(store) => store,
            Err(result) => return result,
        };
    let root = match crate::root::resolve_central_root(context.root_options) {
        Ok(root) => root,
        Err(message) => return failure(message),
    };
    let project = input
        .get("project")
        .and_then(Value::as_str)
        .filter(|_| input["scope"] == "project");
    let (scope_ref, sets) = match project {
        Some(project) => {
            match crate::continuous_work::source::Scope::resolve(&root.path, Some(project)) {
                Ok(scope) => (
                    scope.world_ref,
                    RelationRecordStore::agent_sets_in_project(
                        root.path.join("Work").join(project),
                    ),
                ),
                Err(error) => return failure(error.to_string()),
            }
        }
        None => (
            "control:root".to_owned(),
            RelationRecordStore::agent_sets_at_root(&root.path),
        ),
    };
    let mut subjects = Vec::new();
    let mut absences = Vec::new();

    // The person: your world's one nara subject, from the identity manifest. A project's world names none of its own.
    if project.is_none() {
        if !root.path.join(PASU_IDENTITY_MANIFEST_PATH).is_file() {
            absences.push(format!(
                "No identity manifest at {PASU_IDENTITY_MANIFEST_PATH}: your world names no person subject yet."
            ));
        } else {
            match PasuIdentityManifest::load(&root.path) {
                Ok(manifest) => subjects.push(json!({
                    "ref": manifest.subject.ref_.as_str(),
                    "form": manifest.subject.ref_.form().as_str(),
                    "title": manifest.subject.title.clone().unwrap_or_else(|| "You".to_owned()),
                    "kind": "person",
                    "source": {"record": "identity-manifest", "path": PASU_IDENTITY_MANIFEST_PATH, "revision": manifest.revision},
                })),
                Err(error) => absences.push(format!("The {error}")),
            }
        }
    }

    // The agents: one subject per agent the register's profiles name, each with the profiles it stands on.
    match profiles.list() {
        Ok(readings) => {
            let mut agents: std::collections::BTreeMap<String, Vec<Value>> =
                std::collections::BTreeMap::new();
            let mut titles: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for reading in readings {
                let profile = &reading.profile;
                let title = profile
                    .name
                    .clone()
                    .or_else(|| profile.role.clone())
                    .or_else(|| profile.purpose.clone())
                    .map(|text| text.chars().take(80).collect::<String>())
                    .unwrap_or_else(|| profile.agent_ref.clone());
                titles.entry(profile.agent_ref.clone()).or_insert(title);
                agents
                    .entry(profile.agent_ref.clone())
                    .or_default()
                    .push(json!({
                        "profile_ref": profile.profile_ref,
                        "path": reading.source_path,
                    }));
            }
            for (agent_ref, profiles) in agents {
                match PasuRef::for_agent(&agent_ref) {
                    Ok(pasu) => subjects.push(json!({
                        "ref": pasu.as_str(),
                        "form": "agent",
                        "title": titles.get(&agent_ref).cloned().unwrap_or_else(|| agent_ref.clone()),
                        "kind": "agent",
                        "agent_ref": agent_ref,
                        "source": {"record": "agent-profile", "profiles": profiles},
                    })),
                    Err(error) => absences.push(format!("{agent_ref}: {error}")),
                }
            }
        }
        Err(error) => absences.push(format!("The agent profiles cannot be read: {error:?}")),
    }

    // The agent sets: a group of agents is a bounded entity of its own.
    match sets.list() {
        Ok(readings) => {
            for reading in readings {
                match PasuRef::for_agent_set(&reading.ref_) {
                    Ok(pasu) => subjects.push(json!({
                        "ref": pasu.as_str(),
                        "form": "agent-set",
                        "title": reading.ref_,
                        "kind": "agent-set",
                        "agent_set_ref": reading.ref_,
                        "source": {"record": "agent-set", "path": reading.source_path, "revision": reading.revision},
                    })),
                    Err(error) => absences.push(format!("{}: {error}", reading.ref_)),
                }
            }
        }
        Err(error) => absences.push(format!("The agent sets cannot be read: {error}")),
    }

    ActionResult::success(
        WORLD_SUBJECTS_ACTION,
        json!({
            "schema": WORLD_SUBJECTS_SCHEMA,
            "scope_ref": scope_ref,
            "subjects": subjects,
            // A Thing's subject is an object: Central keeps no record of objects, so none is listed and none is guessed.
            "objects": null,
            "objects_absence": "Central keeps no record of objects: a Thing's subject has no owner listing.",
            "absences": absences,
        }),
    )
}

pub fn register_world_subjects_action(registry: &mut ActionRegistry) {
    let _ = registry.register(
        ActionDescriptor {
            id: WORLD_SUBJECTS_ACTION.into(),
            title: "List a world's subjects".into(),
            description: "List the bounded entities a world holds in the central.pasu/v1 grammar — the person in your world, \
                          each agent its profiles name and each agent set — each with the record it stands on."
                .into(),
            inputs: vec![string_input("scope", true), string_input("project", false)],
            output: ActionOutputDefinition {
                output_type: WORLD_SUBJECTS_SCHEMA.into(),
            },
            mutation_class: MutationClass::ReadOnly,
            preview_supported: false,
            required_ports: Vec::new(),
            availability: ActionAvailability {
                available: true,
                reason: None,
            },
        },
        world_subjects,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::{initialize_central, RootOptions};
    use central_connector_sdk::ConnectorContext;
    use std::fs;

    fn run(root: &std::path::Path, input: Value) -> ActionResult {
        let options = RootOptions {
            explicit_root: Some(root.to_path_buf()),
            configured_root: None,
            home: None,
        };
        let connectors = central_connector_sdk::ConnectorRegistry::default();
        let connector_context = ConnectorContext {
            platform: "test".into(),
        };
        let context = ActionExecutionContext {
            root_options: &options,
            connectors: &connectors,
            connector_context: &connector_context,
        };
        crate::cli::create_runtime_action_registry().execute(
            WORLD_SUBJECTS_ACTION,
            &input,
            &context,
        )
    }

    #[test]
    fn your_world_lists_the_person_each_profiled_agent_and_each_agent_set() {
        let root =
            std::env::temp_dir().join(format!("central-world-subjects-{}", uuid::Uuid::new_v4()));
        initialize_central(&root).unwrap();

        // Nothing authored yet: no subject, and the missing manifest is named.
        let empty = run(&root, json!({"scope": "root"}));
        assert!(empty.ok, "{empty:?}");
        let data = empty.data.unwrap();
        assert_eq!(data["schema"], WORLD_SUBJECTS_SCHEMA);
        assert_eq!(data["scope_ref"], "control:root");
        assert_eq!(data["subjects"], json!([]));
        assert!(data["absences"].to_string().contains("identity manifest"));
        assert_eq!(data["objects"], Value::Null);

        // The person, two profiles of one agent, and one agent set.
        let manifest = PasuIdentityManifest::new(
            crate::pasu::PasuSubject {
                ref_: PasuRef::local_nara(),
                title: Some("Frank".into()),
            },
            crate::pasu::PasuIdentitySource {
                path: "Control/user/identity".into(),
                provenance_law: "authored".into(),
                sources: Vec::new(),
            },
        );
        manifest.persist(&root).unwrap();
        let store = crate::agent_profile_store::AgentProfileStore::personal(&root);
        for (profile_ref, name) in [
            ("agent-profile:reader-a", "Reader"),
            ("agent-profile:reader-b", "Reader, second"),
        ] {
            let mut profile = crate::agent_profile::AgentProfile::propose_from_intent(
                profile_ref,
                "r1",
                "agent:reader",
                crate::agent_profile::AgentProfileScope::Personal,
                crate::world::WorldRef::new("world:personal").unwrap(),
                "Read the selected sources and return what they say.",
                "agent-profile.express",
            )
            .unwrap();
            profile.name = Some(name.into());
            store.save(&profile, None).unwrap();
        }
        let sets = RelationRecordStore::agent_sets_at_root(&root);
        let set = crate::world::AgentSetRecord::new(
            crate::world::AgentSetRef::new("agent-set:readers").unwrap(),
            "r1",
        );
        sets.save(&serde_json::to_value(&set).unwrap(), None)
            .unwrap();

        let listed = run(&root, json!({"scope": "root"}));
        assert!(listed.ok, "{listed:?}");
        let data = listed.data.unwrap();
        let subjects = data["subjects"].as_array().unwrap();
        assert_eq!(subjects[0]["ref"], "central:pasu:nara:local");
        assert_eq!(subjects[0]["title"], "Frank");
        assert_eq!(subjects[0]["kind"], "person");
        let agent = subjects.iter().find(|row| row["form"] == "agent").unwrap();
        assert_eq!(agent["ref"], "central:pasu:agent:agent:reader");
        assert_eq!(
            agent["source"]["profiles"].as_array().unwrap().len(),
            2,
            "one subject, both profiles"
        );
        let set = subjects
            .iter()
            .find(|row| row["form"] == "agent-set")
            .unwrap();
        assert_eq!(set["ref"], "central:pasu:agent-set:agent-set:readers");
        assert_eq!(
            set["title"], "agent-set:readers",
            "an agent set names no title of its own"
        );
        assert_eq!(set["source"]["revision"], "r1");
        assert_eq!(data["absences"], json!([]), "{data}");
        assert_eq!(
            subjects.iter().filter(|row| row["form"] == "agent").count(),
            1
        );

        // An unknown register is refused in the profiles' own words.
        let refused = run(&root, json!({"scope": "elsewhere"}));
        assert!(!refused.ok);
        let _ = fs::remove_dir_all(root);
    }
}
