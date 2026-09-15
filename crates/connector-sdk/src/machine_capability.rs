//! The capability-string convention for `MachineInspectionOutput.capabilities`.
//!
//! The MachineInspector Port types capabilities as free strings
//! (`Vec<String>`). Connectors that observe machine capabilities from
//! external read-models (harness detection, harness-instance registries)
//! encode the observation's source in the string so consumers can always
//! answer "where does this capability claim come from":
//!
//! ```text
//! <name>@source:<source-kind>
//! ```
//!
//! Source kinds used by shipped Connectors:
//!
//! - `actuation-harness-capability` — the harness is detected on this
//!   machine by the Actuation CLI and has a declared capability descriptor.
//! - `actuation-harness-detection` — the harness is detected by the Actuation
//!   CLI but no capability descriptor is declared for it.
//! - `workcell-harness-instance` — a live harness instance is recorded in the
//!   Workcell harness-instance registry.
//! - `absent` — the named source system is not reachable on this machine; the
//!   name records which source was consulted (e.g.
//!   `actuation-harness-detect@source:absent`). This is a disclosure note, not
//!   a capability claim.
//! - `unavailable` — the source system is reachable but the read-model query
//!   failed, was refused, or timed out.
//!
//! Strings without the separator stay valid: they are bare capability names
//! (typically Port identifiers such as `MachineInspector`, reported by
//! platform Connectors). Matching is defined on the name portion, so a bare
//! authored name and a sourced observation of the same capability match.
//!
//! This convention is the backward-compatible encoding for the current Port
//! type. Structured capability records (name plus structured provenance in
//! place of `Vec<String>`) are the recognised upstream evolution of the
//! MachineInspector Port.
//!
//! The upstream sources of these read-models are the Actuation CLI
//! (`actuation harness detect --json`, `actuation harness capability --json`,
//! documents `actuation.harness-detection/v1` and
//! `actuation.harness-capability/v1`) and the Workcell harness-instance
//! registry (`workcell.registry/v1` holding `workcell.harness-instance/v1`
//! records).

/// The separator that introduces the source kind in a capability string.
pub const CAPABILITY_SOURCE_SEPARATOR: &str = "@source:";

pub const SOURCE_ACTUATION_CAPABILITY: &str = "actuation-harness-capability";
pub const SOURCE_ACTUATION_DETECTION: &str = "actuation-harness-detection";
pub const SOURCE_WORKCELL_INSTANCE: &str = "workcell-harness-instance";
pub const SOURCE_ABSENT: &str = "absent";
pub const SOURCE_UNAVAILABLE: &str = "unavailable";

/// The disclosure name reported when the Actuation CLI is not reachable or
/// its harness read-models cannot be read.
pub const ACTUATION_DETECT_DISCLOSURE_NAME: &str = "actuation-harness-detect";

/// The disclosure name reported when the Workcell harness-instance registry
/// is not reachable or cannot be read.
pub const WORKCELL_REGISTRY_DISCLOSURE_NAME: &str = "workcell-harness-instances";

/// The name portion of a capability string: the part before the source
/// separator, or the whole string when no separator is present.
pub fn capability_name(capability: &str) -> &str {
    match capability.split_once(CAPABILITY_SOURCE_SEPARATOR) {
        Some((name, _source)) => name,
        None => capability,
    }
}

/// Compose a capability string carrying its source kind.
pub fn with_source(name: &str, source: &str) -> String {
    format!("{name}{CAPABILITY_SOURCE_SEPARATOR}{source}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_extracts_the_part_before_the_source_separator() {
        assert_eq!(
            capability_name("codex@source:actuation-harness-capability"),
            "codex"
        );
        assert_eq!(capability_name("MachineInspector"), "MachineInspector");
        assert_eq!(capability_name("hermes@source:absent"), "hermes");
    }

    #[test]
    fn with_source_composes_the_documented_convention() {
        assert_eq!(
            with_source("hermes", SOURCE_WORKCELL_INSTANCE),
            "hermes@source:workcell-harness-instance"
        );
        assert_eq!(
            with_source(ACTUATION_DETECT_DISCLOSURE_NAME, SOURCE_ABSENT),
            "actuation-harness-detect@source:absent"
        );
    }
}
