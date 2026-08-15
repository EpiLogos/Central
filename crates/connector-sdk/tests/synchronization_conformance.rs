use central_connector_sdk::{
    run_synchronizer_conformance, CapabilityProbe, Connector, ConnectorContext, ConnectorManifest,
    ConnectorPortDeclaration, PortContract, PortError, StateChangePreview, StateChangeResult,
    SynchronizationRequest, Synchronizer, SynchronizerConformanceFixture, CONNECTOR_API_VERSION,
    SYNCHRONIZER_PORT,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct State {
    changed: Arc<Mutex<bool>>,
}

impl State {
    fn new(changed: bool) -> Self {
        Self {
            changed: Arc::new(Mutex::new(changed)),
        }
    }
}

struct FixtureSynchronizer {
    manifest: ConnectorManifest,
    state: State,
    report_apply_change: bool,
}

impl FixtureSynchronizer {
    fn new(changed: bool, report_apply_change: bool) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "test.synchronizer-conformance".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Synchronizer conformance regression fixture".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: SYNCHRONIZER_PORT.id.to_owned(),
                    version: SYNCHRONIZER_PORT.version.to_owned(),
                }],
                platforms: vec!["*".to_owned()],
                entrypoint: "test:synchronization_conformance::FixtureSynchronizer".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "externally-mutating".to_owned(),
            },
            state: State::new(changed),
            report_apply_change,
        }
    }
}

impl Synchronizer for FixtureSynchronizer {
    fn preview(&self, _input: &SynchronizationRequest) -> Result<StateChangePreview, PortError> {
        let changed = *self.state.changed.lock().unwrap();
        Ok(StateChangePreview {
            changed,
            summary: if changed {
                "fixture would synchronize".to_owned()
            } else {
                "fixture already synchronized".to_owned()
            },
        })
    }

    fn apply(&self, _input: &SynchronizationRequest) -> Result<StateChangeResult, PortError> {
        let mut changed = self.state.changed.lock().unwrap();
        let was_changed = *changed;
        *changed = false;
        Ok(StateChangeResult {
            changed: was_changed && self.report_apply_change,
            summary: "fixture synchronization applied".to_owned(),
        })
    }
}

impl Connector for FixtureSynchronizer {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn synchronizer(&self) -> Option<&dyn Synchronizer> {
        Some(self)
    }
}

fn fixture() -> SynchronizerConformanceFixture {
    SynchronizerConformanceFixture {
        platform: "linux".to_owned(),
        request: SynchronizationRequest {
            id: "fixture".to_owned(),
            source: None,
        },
    }
}

#[test]
fn satisfied_fixture_cannot_false_positive_mutating_conformance() {
    let connector = FixtureSynchronizer::new(false, true);
    let error = run_synchronizer_conformance(&connector, &fixture()).unwrap_err();
    assert!(error.contains("fixture-precondition"), "{error}");
}

#[test]
fn apply_must_report_the_mutation_that_conformance_required() {
    let connector = FixtureSynchronizer::new(true, false);
    let error = run_synchronizer_conformance(&connector, &fixture()).unwrap_err();
    assert!(error.contains("typed-apply"), "{error}");
}
