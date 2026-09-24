//! The machine harness-capability Connector.
//!
//! A read-only MachineInspector that gives the machine record a real
//! capability source. It composes:
//!
//! - platform facts (operating system and architecture, as the platform
//!   Connectors report them), and
//! - harness capabilities sourced from the Actuation CLI
//!   (`actuation harness detect --json` proves which harnesses exist;
//!   `actuation harness capability --json` declares which detected harnesses
//!   carry a capability descriptor), and
//! - live harness-instance records from the Workcell harness-instance
//!   registry (`~/.workcell/instances/registry.json`, schema
//!   `workcell.registry/v1`).
//!
//! Every capability string carries its source under the SDK capability
//! convention (see `central_connector_sdk::machine_capability`); absence of a
//! source system is disclosed explicitly (`<source>@source:absent`) rather
//! than left silent, and a reachable-but-failing source is disclosed as
//! `@source:unavailable`. Inspection never fails: graceful absence is a valid
//! observation.
//!
//! Selection: the ConnectorRegistry resolves the lexicographically smallest
//! eligible Connector for a Port. This Connector's manifest id sorts after
//! the platform Connectors (`personal.*`) so host surfaces keep their rich
//! platform inspectors, and before the reference inspector
//! (`reference.machine-host`) so plain `ctrl` deployments upgrade to real
//! capability disclosure when a source is reachable. Its capability probe
//! reports unavailable when neither the Actuation CLI nor the Workcell
//! registry can be found, so machines without harness sources keep today's
//! behaviour unchanged.
//!
//! Environment overrides (mirroring the platform-Connector precedents):
//! `CENTRAL_ACTUATION_EXECUTABLE` pins the Actuation CLI path (otherwise the
//! executable `actuation` is looked up on `PATH`); `CENTRAL_WORKCELL_REGISTRY`
//! pins the registry file (otherwise `$HOME/.workcell/instances/registry.json`).

use central_connector_sdk::{
    machine_capability::{
        with_source, ACTUATION_DETECT_DISCLOSURE_NAME, SOURCE_ABSENT, SOURCE_ACTUATION_CAPABILITY,
        SOURCE_ACTUATION_DETECTION, SOURCE_UNAVAILABLE, SOURCE_WORKCELL_INSTANCE,
        WORKCELL_REGISTRY_DISCLOSURE_NAME,
    },
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    MachineInspectionInput, MachineInspectionOutput, MachineInspector, PortContract, PortError,
    CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const CONNECTOR_ID: &str = "read-models.harness-capability";

const ACTUATION_EXECUTABLE_NAME: &str = "actuation";
const WORKCELL_REGISTRY_DEFAULT_SUFFIX: &str = ".workcell/instances/registry.json";
const WORKCELL_REGISTRY_SCHEMA: &str = "workcell.registry/v1";
const WORKCELL_HARNESS_REF_PREFIX: &str = "harness/";
const WORKCELL_LIVE: &str = "live";

/// Time budget for one Actuation read-model invocation. The detection
/// document is bounded (filesystem probes for the bundled catalog), and the
/// capability catalog is a pure read-model projection.
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// `ETXTBSY` ("text file busy") is errno 26 on Linux and Darwin. `std::io`
/// has no `ErrorKind` for it, so we match the raw OS error.
const ETXTBSY: i32 = 26;

/// Total spawn attempts before a persistent `ETXTBSY` is surfaced. A single
/// clone+execve window closes within a scheduler turn (see
/// [`spawn_tolerating_text_file_busy`]), so one or two attempts converge in
/// practice; the ceiling only guards against pathology and is never a wait.
const SPAWN_ATTEMPTS_ON_TEXT_FILE_BUSY: u32 = 128;

pub struct HarnessCapabilityConnector {
    manifest: ConnectorManifest,
    actuation_executable: Option<PathBuf>,
    workcell_registry: PathBuf,
    command_timeout: Duration,
}

impl HarnessCapabilityConnector {
    /// Discover the capability sources from the environment: the Actuation
    /// CLI on `PATH` (or `CENTRAL_ACTUATION_EXECUTABLE`) and the Workcell
    /// harness-instance registry under the home directory (or
    /// `CENTRAL_WORKCELL_REGISTRY`).
    pub fn new() -> Self {
        Self::with_sources(discover_actuation(), discover_workcell_registry())
    }

    /// Explicit source paths; `None` marks Actuation as absent. Used by
    /// tests and embeddings that must not depend on the host environment.
    pub fn with_sources(actuation_executable: Option<PathBuf>, workcell_registry: PathBuf) -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: CONNECTOR_ID.to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "Machine harness capability read-model".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: MACHINE_INSPECTOR_PORT.id.to_owned(),
                    version: MACHINE_INSPECTOR_PORT.version.to_owned(),
                }],
                platforms: vec!["*".to_owned()],
                entrypoint: "rust:central-harness-connector::HarnessCapabilityConnector"
                    .to_owned(),
                runtime_requirements: vec![
                    "At least one harness capability source: the Actuation CLI or the Workcell harness-instance registry.".to_owned(),
                ],
                dependency_probes: vec![
                    "actuation executable on PATH or CENTRAL_ACTUATION_EXECUTABLE".to_owned(),
                    "Workcell registry at ~/.workcell/instances/registry.json or CENTRAL_WORKCELL_REGISTRY"
                        .to_owned(),
                ],
                configuration_requirements: Vec::new(),
                mutation_scope: "read-only".to_owned(),
                known_limitations: Vec::new(),
            },
            actuation_executable,
            workcell_registry,
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }

    /// Override the per-invocation time budget for Actuation read-model
    /// queries. Tests use this to bound a wedged stub.
    pub fn with_command_timeout(mut self, command_timeout: Duration) -> Self {
        self.command_timeout = command_timeout;
        self
    }

    fn actuation_capabilities(&self) -> Vec<String> {
        let Some(executable) = &self.actuation_executable else {
            return vec![with_source(ACTUATION_DETECT_DISCLOSURE_NAME, SOURCE_ABSENT)];
        };
        let Some(detection) = self.run_read_model(executable, &["harness", "detect", "--json"])
        else {
            return vec![with_source(
                ACTUATION_DETECT_DISCLOSURE_NAME,
                SOURCE_UNAVAILABLE,
            )];
        };
        let detected = match serde_json::from_str::<Value>(&detection) {
            Ok(value) => detected_harness_slugs(&value),
            Err(_) => {
                return vec![with_source(
                    ACTUATION_DETECT_DISCLOSURE_NAME,
                    SOURCE_UNAVAILABLE,
                )]
            }
        };
        if detected.is_empty() {
            return Vec::new();
        }
        // One catalog query classifies every detected harness at once; a
        // failing or refused catalog query demotes all harnesses to
        // detection-sourced disclosure rather than failing the inspection.
        let capable = self
            .run_read_model(executable, &["harness", "capability", "--json"])
            .and_then(|catalog| serde_json::from_str::<Value>(&catalog).ok())
            .map(|value| capable_harness_slugs(&value))
            .unwrap_or_default();
        detected
            .into_iter()
            .map(|slug| {
                if capable.contains(&slug) {
                    with_source(&slug, SOURCE_ACTUATION_CAPABILITY)
                } else {
                    with_source(&slug, SOURCE_ACTUATION_DETECTION)
                }
            })
            .collect()
    }

    fn workcell_capabilities(&self) -> Vec<String> {
        if !self.workcell_registry.is_file() {
            return vec![with_source(
                WORKCELL_REGISTRY_DISCLOSURE_NAME,
                SOURCE_ABSENT,
            )];
        }
        let parsed = std::fs::read_to_string(&self.workcell_registry)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let Some(registry) = parsed else {
            return vec![with_source(
                WORKCELL_REGISTRY_DISCLOSURE_NAME,
                SOURCE_UNAVAILABLE,
            )];
        };
        if registry.get("schema").and_then(Value::as_str) != Some(WORKCELL_REGISTRY_SCHEMA) {
            return vec![with_source(
                WORKCELL_REGISTRY_DISCLOSURE_NAME,
                SOURCE_UNAVAILABLE,
            )];
        }
        let mut capabilities: Vec<String> = registry
            .get("instances")
            .and_then(Value::as_object)
            .map(|instances| {
                instances
                    .values()
                    .filter(|record| {
                        record.get("liveness").and_then(Value::as_str) == Some(WORKCELL_LIVE)
                    })
                    .filter_map(harness_name)
                    .map(|name| with_source(&name, SOURCE_WORKCELL_INSTANCE))
                    .collect()
            })
            .unwrap_or_default();
        capabilities.sort();
        capabilities.dedup();
        capabilities
    }

    fn run_read_model(&self, executable: &Path, args: &[&str]) -> Option<String> {
        let mut child = spawn_tolerating_text_file_busy(|| {
            Command::new(executable)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        })
        .ok()?;
        let deadline = Instant::now() + self.command_timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => return None,
            }
        };
        let status = status?;
        if !status.success() {
            return None;
        }
        // Bounded read: read-model documents are small; a source emitting
        // beyond the pipe budget trips the timeout above instead of hanging.
        let mut stdout = String::new();
        child.stdout.take()?.read_to_string(&mut stdout).ok()?;
        Some(stdout)
    }
}

impl Default for HarnessCapabilityConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a child, tolerating a transient `ETXTBSY`.
///
/// On Linux `posix_spawn` is a userspace clone+execve. While one thread still
/// holds a freshly written executable open for writing, another thread's spawn
/// can `fork` and inherit that writer descriptor; the target inode then stays
/// open-for-write across the child's `execve`, which fails with `ETXTBSY`
/// ("text file busy"). The inheriting child drops the descriptor at its own
/// `execve` — every descriptor Rust opens is `O_CLOEXEC` — so the condition
/// clears within a scheduler turn. Darwin's `posix_spawn` is a single atomic
/// syscall with no such window, which is why this only surfaces under parallel
/// load on Linux; a freshly installed or updated Actuation binary can trip the
/// same condition in production.
///
/// We retry a bounded number of times, yielding the CPU (never sleeping) so the
/// inheriting child can reach its `execve` and release the descriptor. Any
/// other error is surfaced on the first attempt, unretried.
fn spawn_tolerating_text_file_busy<T>(
    mut attempt: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut remaining = SPAWN_ATTEMPTS_ON_TEXT_FILE_BUSY;
    loop {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                remaining -= 1;
                if remaining > 0 && error.raw_os_error() == Some(ETXTBSY) {
                    std::thread::yield_now();
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Detected harness slugs from an `actuation.harness-detection/v1` document;
/// `not-installed` and `unavailable` harnesses are not capabilities.
fn detected_harness_slugs(detection: &Value) -> Vec<String> {
    let mut slugs: Vec<String> = detection
        .get("harnesses")
        .and_then(Value::as_array)
        .map(|harnesses| {
            harnesses
                .iter()
                .filter(|harness| harness.get("state").and_then(Value::as_str) == Some("detected"))
                .filter_map(|harness| harness.get("slug").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    slugs.sort();
    slugs.dedup();
    slugs
}

/// Harness slugs with a declared capability descriptor from an
/// `actuation.harness-capability/v1` catalog document.
fn capable_harness_slugs(catalog: &Value) -> Vec<String> {
    catalog
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|capabilities| {
            capabilities
                .iter()
                .filter_map(|capability| capability.get("harness_slug").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The readable harness name of a `workcell.harness-instance/v1` record:
/// the `harness_ref` without its `harness/` prefix.
fn harness_name(record: &Value) -> Option<String> {
    let harness_ref = record.get("harness_ref").and_then(Value::as_str)?;
    let name = harness_ref
        .strip_prefix(WORKCELL_HARNESS_REF_PREFIX)
        .unwrap_or(harness_ref);
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn discover_actuation() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("CENTRAL_ACTUATION_EXECUTABLE") {
        return Some(PathBuf::from(explicit));
    }
    let path_variable = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path_variable) {
        let candidate = directory.join(ACTUATION_EXECUTABLE_NAME);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn discover_workcell_registry() -> PathBuf {
    if let Some(explicit) = std::env::var_os("CENTRAL_WORKCELL_REGISTRY") {
        return PathBuf::from(explicit);
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    let mut path = PathBuf::from(home);
    for component in WORKCELL_REGISTRY_DEFAULT_SUFFIX.split('/') {
        path.push(component);
    }
    path
}

impl MachineInspector for HarnessCapabilityConnector {
    fn inspect(
        &self,
        _input: &MachineInspectionInput,
    ) -> Result<MachineInspectionOutput, PortError> {
        let mut capabilities = vec![MACHINE_INSPECTOR_PORT.id.to_owned()];
        capabilities.extend(self.actuation_capabilities());
        capabilities.extend(self.workcell_capabilities());
        capabilities.sort();
        capabilities.dedup();
        Ok(MachineInspectionOutput {
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            capabilities,
            packages: Vec::new(),
            configurations: Vec::new(),
            services: Vec::new(),
        })
    }
}

impl Connector for HarnessCapabilityConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        if port.id != MACHINE_INSPECTOR_PORT.id || port.version != MACHINE_INSPECTOR_PORT.version {
            return CapabilityProbe::unavailable(format!(
                "Harness capability Connector implements only {} {}.",
                MACHINE_INSPECTOR_PORT.id, MACHINE_INSPECTOR_PORT.version
            ));
        }
        if self.actuation_executable.is_some() || self.workcell_registry.is_file() {
            return CapabilityProbe::available();
        }
        CapabilityProbe::unavailable(format!(
            "No harness capability source is reachable: the Actuation CLI ('{ACTUATION_EXECUTABLE_NAME}') was not found on PATH and the Workcell instance registry is missing at {}.",
            self.workcell_registry.display()
        ))
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }
}

#[cfg(test)]
mod spawn_retry {
    use super::{spawn_tolerating_text_file_busy, ETXTBSY, SPAWN_ATTEMPTS_ON_TEXT_FILE_BUSY};
    use std::cell::Cell;
    use std::io;

    // ENOENT: a spawn error that is not a transient text-file-busy race.
    const ENOENT: i32 = 2;

    #[test]
    fn transient_text_file_busy_is_retried_until_the_spawn_succeeds() {
        let attempts = Cell::new(0u32);
        let result = spawn_tolerating_text_file_busy(|| {
            let seen = attempts.get();
            attempts.set(seen + 1);
            if seen < 3 {
                Err(io::Error::from_raw_os_error(ETXTBSY))
            } else {
                Ok("spawned")
            }
        });
        assert_eq!(result.unwrap(), "spawned");
        assert_eq!(attempts.get(), 4, "three ETXTBSY failures, then success");
    }

    #[test]
    fn other_spawn_errors_are_surfaced_without_retrying() {
        let attempts = Cell::new(0u32);
        let result: io::Result<()> = spawn_tolerating_text_file_busy(|| {
            attempts.set(attempts.get() + 1);
            Err(io::Error::from_raw_os_error(ENOENT))
        });
        assert_eq!(result.unwrap_err().raw_os_error(), Some(ENOENT));
        assert_eq!(attempts.get(), 1, "a non-ETXTBSY error is not retried");
    }

    #[test]
    fn a_persistent_text_file_busy_is_bounded_and_then_surfaced() {
        let attempts = Cell::new(0u32);
        let result: io::Result<()> = spawn_tolerating_text_file_busy(|| {
            attempts.set(attempts.get() + 1);
            Err(io::Error::from_raw_os_error(ETXTBSY))
        });
        assert_eq!(result.unwrap_err().raw_os_error(), Some(ETXTBSY));
        assert_eq!(
            attempts.get(),
            SPAWN_ATTEMPTS_ON_TEXT_FILE_BUSY,
            "retries are bounded rather than unbounded"
        );
    }
}
