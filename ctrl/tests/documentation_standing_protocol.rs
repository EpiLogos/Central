use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn standing_contract() -> Value {
    let bytes = fs::read(repository_root().join("docs/documentation-standing.v1.json")).unwrap();
    serde_json::from_slice(&bytes).expect("documentation standing contract must be valid JSON")
}

#[test]
fn documentation_standing_contract_has_exact_canonical_six() {
    let contract = standing_contract();
    assert_eq!(
        contract.get("schema").and_then(Value::as_str),
        Some("central.documentation-standing/v1")
    );

    let standings = contract
        .get("standings")
        .and_then(Value::as_array)
        .expect("standings must be an array");
    let ids = standings
        .iter()
        .map(|standing| {
            standing
                .get("id")
                .and_then(Value::as_str)
                .expect("standing id")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "authored-human-position",
            "design-commitment",
            "architecture-contract",
            "implementation-fact",
            "observed-evidence",
            "agent-inference",
        ]
    );
    assert!(!ids.contains(&"current-development-state"));

    for (expected_order, standing) in standings.iter().enumerate() {
        assert_eq!(
            standing.get("order").and_then(Value::as_u64),
            Some(expected_order as u64),
            "standing order must remain explicit and deterministic"
        );
    }
}

#[test]
fn project_act_cycle_is_explicitly_orthogonal_to_documentation_standing() {
    let contract = standing_contract();
    assert_eq!(
        contract
            .get("standing_is_not_project_act_position")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        contract
            .get("standing_is_not_precedence")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        contract
            .get("filename_establishes_standing")
            .and_then(Value::as_bool),
        Some(false)
    );

    let act_positions = contract
        .pointer("/orthogonal_dimensions/project_act_position")
        .and_then(Value::as_array)
        .expect("Project-act positions must be represented separately");
    let act_positions = act_positions
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        act_positions,
        vec![
            "P0-ground",
            "P1-world",
            "P2-praxis",
            "P3-intent",
            "P4-context-frame",
            "P5-return-recognition",
        ]
    );
}

#[test]
fn current_development_state_is_temporal_lifecycle_context_not_standing() {
    let contract = standing_contract();
    let non_standing = contract
        .get("non_standing_values")
        .and_then(Value::as_array)
        .expect("non-standing values");
    assert!(non_standing
        .iter()
        .any(|value| value.as_str() == Some("current-development-state")));

    let temporal = contract
        .pointer("/orthogonal_dimensions/temporal_development_state")
        .and_then(Value::as_array)
        .expect("temporal development state examples");
    assert!(!temporal.is_empty());

    let protocol = fs::read_to_string(repository_root().join("docs/PROJECT-CONTEXT-PROTOCOL.md"))
        .unwrap();
    assert!(protocol.contains("`current-development-state` is **not a seventh documentation standing**"));
    assert!(protocol.contains("**documentation standing != Project-act position**"));
}

#[test]
fn documentation_steward_skill_consumes_the_same_ladder_and_refuses_p1_flattening() {
    let skill = fs::read_to_string(
        repository_root().join("skills/documentation-standing/SKILL.md"),
    )
    .unwrap();

    let ordered_markers = [
        "authored position",
        "design commitment",
        "architecture contract",
        "implementation fact",
        "observed evidence",
        "Agent inference",
    ];
    let mut cursor = 0usize;
    for marker in ordered_markers {
        let relative = skill[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("documentation steward is missing {marker}"));
        cursor += relative + marker.len();
    }

    assert!(skill.contains("Filename is a discovery hint, not standing."));
    assert!(skill.contains("must not be used to flatten documentation into P1 `World`"));
    assert!(skill.contains("Do not manufacture missing intermediate layers"));
}
