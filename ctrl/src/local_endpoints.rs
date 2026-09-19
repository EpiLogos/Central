use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::projectcentral::{read_project_manifest, PROJECTCENTRAL_DIR, PROJECT_MANIFEST};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROJECT_LOCAL_ENDPOINTS_SOURCE: &str = "ProjectCentral/local-endpoints.json";
pub const PROJECT_LOCAL_ENDPOINTS_SCHEMA: &str = "central.project.local-endpoints/v2";
/// v1 sources stay readable for compatibility: their scope vocabulary is
/// fixed to `localhost`, and the first mutation normalises them onto v2.
pub const PROJECT_LOCAL_ENDPOINTS_SCHEMA_V1: &str = "central.project.local-endpoints/v1";
pub const PROJECT_LOCAL_ENDPOINTS_INSPECTION_SCHEMA: &str =
    "central.project.local-endpoints-inspection/v2";
pub const CENTRAL_LOCAL_ENDPOINTS_SCHEMA: &str = "central.local-endpoint-registry/v2";
/// Loopback only (`127.0.0.1`, `::1`). The v1 scope and the default.
pub const SCOPE_LOCALHOST: &str = "localhost";
/// The machine's tailnet interfaces: Tailscale IPv4 CGNAT `100.64.0.0/10`
/// and Tailscale IPv6 ULA `fd7a:115c:a1e0::/48` addresses.
pub const SCOPE_TAILNET: &str = "tailnet";
/// Wildcard binding (`0.0.0.0`, `::`), reachable on every interface.
pub const SCOPE_ANY: &str = "any";
pub const LOCAL_ENDPOINT_SCOPES: [&str; 3] = [SCOPE_LOCALHOST, SCOPE_TAILNET, SCOPE_ANY];
pub const CENTRAL_LOCAL_ENDPOINTS_CACHE: &str = ".central/local-endpoints.json";
const CENTRAL_LOCAL_ENDPOINTS_LOCK: &str = ".central/local-endpoints.lock";
const LOCK_STALE_SECONDS: u64 = 30;
const DEFAULT_SUGGEST_START: u16 = 3000;
const DEFAULT_SUGGEST_END: u16 = 9999;
const MAX_SUGGEST_SPAN: u32 = 10_000;

fn default_protocol() -> String {
    "tcp".to_owned()
}

fn default_scope() -> String {
    "localhost".to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectLocalEndpoints {
    pub schema: String,
    #[serde(default)]
    pub endpoints: Vec<LocalEndpointDeclaration>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}

impl Default for ProjectLocalEndpoints {
    fn default() -> Self {
        Self {
            schema: PROJECT_LOCAL_ENDPOINTS_SCHEMA.to_owned(),
            endpoints: Vec::new(),
            extensions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalEndpointDeclaration {
    pub id: String,
    pub kind: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}

impl LocalEndpointDeclaration {
    pub fn localhost_tcp(id: impl Into<String>, kind: impl Into<String>, port: u16) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            protocol: default_protocol(),
            scope: default_scope(),
            port,
            service: None,
            description: None,
            extensions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ProjectLocalEndpoints {
    pub fn validate(&self) -> LocalEndpointValidation {
        let mut errors = Vec::new();
        if self.schema != PROJECT_LOCAL_ENDPOINTS_SCHEMA
            && self.schema != PROJECT_LOCAL_ENDPOINTS_SCHEMA_V1
        {
            errors.push(format!(
                "schema must be {PROJECT_LOCAL_ENDPOINTS_SCHEMA} (or {PROJECT_LOCAL_ENDPOINTS_SCHEMA_V1}, read for compatibility)"
            ));
        }

        let mut ids = BTreeSet::new();
        let mut sockets = BTreeSet::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let prefix = format!("endpoints[{index}]");
            if endpoint.id.trim().is_empty() || endpoint.id != endpoint.id.trim() {
                errors.push(format!(
                    "{prefix}.id must be a non-empty stable identity without surrounding whitespace"
                ));
            } else if !ids.insert(endpoint.id.clone()) {
                errors.push(format!("duplicate endpoint id: {}", endpoint.id));
            }

            if endpoint.kind.trim().is_empty() || endpoint.kind != endpoint.kind.trim() {
                errors.push(format!(
                    "{prefix}.kind must be non-empty without surrounding whitespace"
                ));
            }
            if endpoint.protocol != "tcp" {
                errors.push(format!("{prefix}.protocol must be tcp"));
            }
            if self.schema == PROJECT_LOCAL_ENDPOINTS_SCHEMA_V1 {
                if endpoint.scope != SCOPE_LOCALHOST {
                    errors.push(format!(
                        "{prefix}.scope must be localhost in {PROJECT_LOCAL_ENDPOINTS_SCHEMA_V1}; declare interface scopes under {PROJECT_LOCAL_ENDPOINTS_SCHEMA}"
                    ));
                }
            } else if !LOCAL_ENDPOINT_SCOPES.contains(&endpoint.scope.as_str()) {
                errors.push(format!(
                    "{prefix}.scope must be one of {}",
                    LOCAL_ENDPOINT_SCOPES.join(", ")
                ));
            }
            if endpoint.port == 0 {
                errors.push(format!("{prefix}.port must be in 1..=65535"));
            }
            if endpoint
                .service
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value != value.trim())
            {
                errors.push(format!(
                    "{prefix}.service must be non-empty without surrounding whitespace when present"
                ));
            }
            if endpoint
                .description
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value != value.trim())
            {
                errors.push(format!(
                    "{prefix}.description must be non-empty without surrounding whitespace when present"
                ));
            }

            let socket = (
                endpoint.protocol.clone(),
                endpoint.scope.clone(),
                endpoint.port,
            );
            if endpoint.port != 0 && !sockets.insert(socket) {
                errors.push(format!(
                    "duplicate Project-local socket declaration: {}://{}:{}",
                    endpoint.protocol, endpoint.scope, endpoint.port
                ));
            }
        }

        LocalEndpointValidation {
            valid: errors.is_empty(),
            errors,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalEndpointOccupancy {
    Available,
    Occupied,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointObservation {
    pub status: LocalEndpointOccupancy,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservedProjectLocalEndpoint {
    pub declaration: LocalEndpointDeclaration,
    pub observation: LocalEndpointObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectLocalEndpointInspection {
    pub schema: String,
    pub project_root: PathBuf,
    pub project_id: String,
    pub source: String,
    pub source_exists: bool,
    pub valid: bool,
    pub errors: Vec<String>,
    pub endpoints: Vec<ObservedProjectLocalEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalEndpointMutation {
    pub project_root: PathBuf,
    pub source: String,
    pub changed: bool,
    pub endpoint_id: String,
    pub declaration_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointOwner {
    pub project: String,
    pub project_id: String,
    pub endpoint_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredLocalEndpointCollision {
    pub protocol: String,
    pub scope: String,
    pub port: u16,
    pub owners: Vec<LocalEndpointOwner>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CentralLocalEndpointReading {
    pub project: String,
    pub project_id: String,
    pub source: String,
    pub declaration: LocalEndpointDeclaration,
    pub observation: LocalEndpointObservation,
    pub declared_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointSourceError {
    pub project: String,
    pub source: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CentralLocalEndpointRegistry {
    pub schema: String,
    pub generated_at_unix_seconds: u64,
    pub endpoints: Vec<CentralLocalEndpointReading>,
    pub collisions: Vec<DeclaredLocalEndpointCollision>,
    pub source_errors: Vec<LocalEndpointSourceError>,
}

/// One scope's observation inside a suggestion payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointScopeObservation {
    pub scope: String,
    pub addresses: Vec<String>,
    pub status: LocalEndpointOccupancy,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEndpointSuggestion {
    pub protocol: String,
    /// The default declaration scope for the suggested port. The port is
    /// verified bindable on every scope listed in `free_in`.
    pub scope: String,
    pub port: u16,
    pub range_start: u16,
    pub range_end: u16,
    /// Scopes where the port was probed and found bindable.
    pub free_in: Vec<LocalEndpointScopeObservation>,
    /// Scopes this machine cannot observe (no interface address), for which
    /// no occupancy claim is made.
    pub unverified_in: Vec<LocalEndpointScopeObservation>,
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn ensure_projectcentral(project_root: &Path) -> io::Result<String> {
    let manifest = read_project_manifest(project_root)?;
    let validation = manifest.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    Ok(manifest.project_id)
}

pub fn read_project_local_endpoints(project_root: &Path) -> io::Result<ProjectLocalEndpoints> {
    let path = project_root.join(PROJECT_LOCAL_ENDPOINTS_SOURCE);
    if !path.exists() {
        return Ok(ProjectLocalEndpoints::default());
    }
    let bytes = fs::read(&path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is not valid local-endpoint JSON: {error}",
                path.display()
            ),
        )
    })
}

fn write_project_local_endpoints(
    project_root: &Path,
    declaration: &ProjectLocalEndpoints,
) -> io::Result<()> {
    let path = project_root.join(PROJECT_LOCAL_ENDPOINTS_SOURCE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Any mutation writes the current schema; v1 sources normalise onto v2,
    // which is a strict vocabulary superset with the same localhost default.
    let mut declaration = declaration.clone();
    declaration.schema = PROJECT_LOCAL_ENDPOINTS_SCHEMA.to_owned();
    let mut bytes = serde_json::to_vec_pretty(&declaration)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

fn loopback_addresses(port: u16) -> Vec<SocketAddr> {
    vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
    ]
}

fn wildcard_addresses(port: u16) -> Vec<SocketAddr> {
    vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port),
    ]
}

/// Tailscale assigns tailnet addresses from its own IPv4 CGNAT block and its
/// own IPv6 ULA block; any other interface address is not tailnet.
fn is_tailnet_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            octets[0] == 100 && (64..=127).contains(&octets[1]) // 100.64.0.0/10
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            segments[0] == 0xfd7a && segments[1] == 0x115c && segments[2] == 0xa1e0
            // fd7a:115c:a1e0::/48
        }
    }
}

/// Enumerate the machine's interface addresses (`ip -o addr show` on Linux,
/// `ifconfig -a` on macOS). An empty or failed discovery is honest: callers
/// report `unknown` rather than inventing occupancy.
pub fn discover_interface_addresses() -> Vec<IpAddr> {
    let Some(listing) = interface_address_listing() else {
        return Vec::new();
    };
    let mut addresses = BTreeSet::new();
    let mut tokens = listing.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        if token != "inet" && token != "inet6" {
            continue;
        }
        if let Some(value) = tokens.peek() {
            // `ip` writes `10.0.0.1/24`; `ifconfig` may suffix `%interface`.
            let candidate = value.split('%').next().unwrap_or(value);
            let candidate = candidate.split('/').next().unwrap_or(candidate);
            if let Ok(address) = candidate.parse::<IpAddr>() {
                addresses.insert(address);
            }
        }
    }
    addresses.into_iter().collect()
}

#[cfg(target_os = "linux")]
fn interface_address_listing() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["-o", "addr", "show"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn interface_address_listing() -> Option<String> {
    let output = std::process::Command::new("ifconfig")
        .arg("-a")
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn interface_address_listing() -> Option<String> {
    None
}

/// The machine's tailnet addresses, discovered from the live interfaces.
pub fn tailnet_interface_addresses() -> Vec<IpAddr> {
    discover_interface_addresses()
        .into_iter()
        .filter(|address| is_tailnet_address(*address))
        .collect()
}

fn scope_addresses_with_tailnet(scope: &str, port: u16, tailnet: &[IpAddr]) -> Vec<SocketAddr> {
    match scope {
        SCOPE_LOCALHOST => loopback_addresses(port),
        SCOPE_TAILNET => tailnet
            .iter()
            .map(|address| SocketAddr::new(*address, port))
            .collect(),
        SCOPE_ANY => wildcard_addresses(port),
        _ => Vec::new(),
    }
}

fn scope_addresses(scope: &str, port: u16) -> Vec<SocketAddr> {
    scope_addresses_with_tailnet(scope, port, &tailnet_interface_addresses())
}

/// Probe a port by briefly binding each address and dropping the listener.
/// The first `AddrInUse` reports `occupied`; addresses that cannot exist on
/// this machine are skipped; anything else is honest `unknown`.
///
/// The probe binds strictly: no `SO_REUSEADDR`, no `SO_REUSEPORT`. Rust's
/// `std` sets `SO_REUSEADDR` on every Unix bind, and the BSD semantics macOS
/// follows let a wildcard bind succeed while a specific address (e.g.
/// `127.0.0.1`) already holds the port — a plain-std probe then reported
/// `available` for a port that nothing else could actually use. Strict
/// semantics give `AddrInUse` its plain meaning on macOS and Linux alike.
pub fn probe_bound_addresses(label: &str, addresses: &[SocketAddr]) -> LocalEndpointObservation {
    if addresses.is_empty() {
        return LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: format!(
                "{label} scope has no observable interface address on this machine; occupancy cannot be established"
            ),
        };
    }

    let mut bindable = false;
    let mut errors = Vec::new();

    for address in addresses {
        match strict_tcp_listener(*address) {
            Ok(listener) => {
                bindable = true;
                drop(listener);
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                return LocalEndpointObservation {
                    status: LocalEndpointOccupancy::Occupied,
                    detail: format!("{address} is currently occupied"),
                };
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::AddrNotAvailable | io::ErrorKind::Unsupported
                ) =>
            {
                // A machine may not expose every probed address family.
            }
            Err(error) => errors.push(format!("{address}: {error}")),
        }
    }

    if !errors.is_empty() {
        LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: errors.join("; "),
        }
    } else if bindable {
        LocalEndpointObservation {
            status: LocalEndpointOccupancy::Available,
            detail: format!("{label} TCP port is bindable on all probed addresses"),
        }
    } else {
        LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: "no supported address family was available for probing".into(),
        }
    }
}

/// Bind a probe listener with strict address semantics: no `SO_REUSEADDR`
/// and no `SO_REUSEPORT`. The occupancy probe needs a bind whose success
/// actually proves the port free on that address, and std's default
/// `SO_REUSEADDR` breaks that proof on macOS (BSD lets a wildcard bind
/// overlap a specific-address bind, so `0.0.0.0` reported free while
/// `127.0.0.1` held the port).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn strict_tcp_listener(address: SocketAddr) -> io::Result<TcpListener> {
    use std::os::fd::FromRawFd;

    let family = match address {
        SocketAddr::V4(_) => libc::AF_INET,
        SocketAddr::V6(_) => libc::AF_INET6,
    };
    // Ownership: on success `fd` is a fresh socket this function owns; every
    // error path closes it before returning.
    let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let bound = match address {
        SocketAddr::V4(v4) => {
            let mut socket_address: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            socket_address.sin_family = libc::AF_INET as libc::sa_family_t;
            #[cfg(target_os = "macos")]
            {
                socket_address.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
            }
            socket_address.sin_port = v4.port().to_be();
            socket_address.sin_addr.s_addr = u32::from_ne_bytes(v4.ip().octets());
            (
                &socket_address as *const libc::sockaddr_in as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        }
        SocketAddr::V6(v6) => {
            let mut socket_address: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            socket_address.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            #[cfg(target_os = "macos")]
            {
                socket_address.sin6_len = std::mem::size_of::<libc::sockaddr_in6>() as u8;
            }
            socket_address.sin6_port = v6.port().to_be();
            socket_address.sin6_addr.s6_addr = v6.ip().octets();
            socket_address.sin6_flowinfo = v6.flowinfo();
            socket_address.sin6_scope_id = v6.scope_id();
            (
                &socket_address as *const libc::sockaddr_in6 as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            )
        }
    };
    let bind_status = unsafe { libc::bind(fd, bound.0, bound.1) };
    if bind_status != 0 {
        let error = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(error);
    }
    let listen_status = unsafe { libc::listen(fd, libc::SOMAXCONN) };
    if listen_status != 0 {
        let error = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(error);
    }
    // `fd` is an owned, listening stream socket; hand it to std for cleanup.
    Ok(unsafe { TcpListener::from_raw_fd(fd) })
}

/// Platforms outside the deployed set keep std's bind semantics; interface
/// discovery already reports `unknown` rather than inventing occupancy there.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn strict_tcp_listener(address: SocketAddr) -> io::Result<TcpListener> {
    TcpListener::bind(address)
}

pub fn probe_local_tcp_port(port: u16) -> LocalEndpointObservation {
    if port == 0 {
        return LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: "port 0 is not a stable endpoint allocation".into(),
        };
    }
    probe_bound_addresses(SCOPE_LOCALHOST, &loopback_addresses(port))
}

/// Probe the scope's actual interfaces for a port: loopback families for
/// `localhost`, discovered tailnet addresses for `tailnet`, wildcard binds
/// for `any`.
pub fn probe_scoped_tcp_port(scope: &str, port: u16) -> LocalEndpointObservation {
    if !LOCAL_ENDPOINT_SCOPES.contains(&scope) {
        return LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: format!(
                "{scope} is not a known endpoint scope; expected one of {}",
                LOCAL_ENDPOINT_SCOPES.join(", ")
            ),
        };
    }
    if port == 0 {
        return LocalEndpointObservation {
            status: LocalEndpointOccupancy::Unknown,
            detail: "port 0 is not a stable endpoint allocation".into(),
        };
    }
    probe_bound_addresses(scope, &scope_addresses(scope, port))
}

pub fn inspect_project_local_endpoints(
    project_root: &Path,
) -> io::Result<ProjectLocalEndpointInspection> {
    let project_id = ensure_projectcentral(project_root)?;
    let source_path = project_root.join(PROJECT_LOCAL_ENDPOINTS_SOURCE);
    let source_exists = source_path.is_file();

    let declaration = match read_project_local_endpoints(project_root) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return Ok(ProjectLocalEndpointInspection {
                schema: PROJECT_LOCAL_ENDPOINTS_INSPECTION_SCHEMA.into(),
                project_root: project_root.to_path_buf(),
                project_id,
                source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
                source_exists,
                valid: false,
                errors: vec![error.to_string()],
                endpoints: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };
    let validation = declaration.validate();
    if !validation.valid {
        return Ok(ProjectLocalEndpointInspection {
            schema: PROJECT_LOCAL_ENDPOINTS_INSPECTION_SCHEMA.into(),
            project_root: project_root.to_path_buf(),
            project_id,
            source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
            source_exists,
            valid: false,
            errors: validation.errors,
            endpoints: Vec::new(),
        });
    }

    let endpoints = declaration
        .endpoints
        .into_iter()
        .map(|declaration| ObservedProjectLocalEndpoint {
            observation: probe_scoped_tcp_port(&declaration.scope, declaration.port),
            declaration,
        })
        .collect();

    Ok(ProjectLocalEndpointInspection {
        schema: PROJECT_LOCAL_ENDPOINTS_INSPECTION_SCHEMA.into(),
        project_root: project_root.to_path_buf(),
        project_id,
        source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
        source_exists,
        valid: true,
        errors: Vec::new(),
        endpoints,
    })
}

pub fn set_project_local_endpoint(
    project_root: &Path,
    mut endpoint: LocalEndpointDeclaration,
) -> io::Result<LocalEndpointMutation> {
    ensure_projectcentral(project_root)?;
    let mut declaration = read_project_local_endpoints(project_root)?;
    // Mutations write the current schema; v1 sources normalise onto v2 so a
    // non-loopback scope can join a file that predated the scope vocabulary.
    declaration.schema = PROJECT_LOCAL_ENDPOINTS_SCHEMA.to_owned();
    let validation = declaration.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }

    let mut changed = true;
    if let Some(existing) = declaration
        .endpoints
        .iter_mut()
        .find(|existing| existing.id == endpoint.id)
    {
        endpoint.extensions = existing.extensions.clone();
        changed = *existing != endpoint;
        if changed {
            *existing = endpoint.clone();
        }
    } else {
        declaration.endpoints.push(endpoint.clone());
    }
    declaration
        .endpoints
        .sort_by(|left, right| left.id.cmp(&right.id));

    let validation = declaration.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            validation.errors.join("; "),
        ));
    }
    if changed {
        write_project_local_endpoints(project_root, &declaration)?;
    }

    Ok(LocalEndpointMutation {
        project_root: project_root.to_path_buf(),
        source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
        changed,
        endpoint_id: endpoint.id,
        declaration_count: declaration.endpoints.len(),
    })
}

pub fn remove_project_local_endpoint(
    project_root: &Path,
    endpoint_id: &str,
) -> io::Result<LocalEndpointMutation> {
    ensure_projectcentral(project_root)?;
    if endpoint_id.trim().is_empty() || endpoint_id != endpoint_id.trim() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "endpoint id must be non-empty without surrounding whitespace",
        ));
    }

    let source_path = project_root.join(PROJECT_LOCAL_ENDPOINTS_SOURCE);
    if !source_path.exists() {
        return Ok(LocalEndpointMutation {
            project_root: project_root.to_path_buf(),
            source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
            changed: false,
            endpoint_id: endpoint_id.into(),
            declaration_count: 0,
        });
    }

    let mut declaration = read_project_local_endpoints(project_root)?;
    let validation = declaration.validate();
    if !validation.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            validation.errors.join("; "),
        ));
    }
    let before = declaration.endpoints.len();
    declaration
        .endpoints
        .retain(|endpoint| endpoint.id != endpoint_id);
    let changed = before != declaration.endpoints.len();
    if changed {
        write_project_local_endpoints(project_root, &declaration)?;
    }

    Ok(LocalEndpointMutation {
        project_root: project_root.to_path_buf(),
        source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
        changed,
        endpoint_id: endpoint_id.into(),
        declaration_count: declaration.endpoints.len(),
    })
}

pub fn inspect_central_local_endpoints(
    central_root: &Path,
) -> io::Result<CentralLocalEndpointRegistry> {
    let work_root = central_root.join("Work");
    let tailnet = tailnet_interface_addresses();
    let mut source_errors = Vec::new();
    let mut endpoints = Vec::new();
    let mut observation_cache: BTreeMap<(String, u16), LocalEndpointObservation> = BTreeMap::new();

    if work_root.is_dir() {
        let mut projects = fs::read_dir(&work_root)?.collect::<Result<Vec<_>, _>>()?;
        projects.sort_by_key(|entry| entry.file_name());
        for entry in projects {
            let project_root = entry.path();
            if !project_root.is_dir()
                || !project_root
                    .join(PROJECTCENTRAL_DIR)
                    .join(PROJECT_MANIFEST)
                    .is_file()
            {
                continue;
            }
            let project = entry.file_name().to_string_lossy().to_string();
            let manifest = match read_project_manifest(&project_root) {
                Ok(manifest) => manifest,
                Err(error) => {
                    source_errors.push(LocalEndpointSourceError {
                        project,
                        source: format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"),
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            let manifest_validation = manifest.validate();
            if !manifest_validation.valid {
                source_errors.push(LocalEndpointSourceError {
                    project,
                    source: format!("{PROJECTCENTRAL_DIR}/{PROJECT_MANIFEST}"),
                    error: manifest_validation.errors.join("; "),
                });
                continue;
            }

            let source_path = project_root.join(PROJECT_LOCAL_ENDPOINTS_SOURCE);
            if !source_path.is_file() {
                continue;
            }
            let declaration = match read_project_local_endpoints(&project_root) {
                Ok(value) => value,
                Err(error) => {
                    source_errors.push(LocalEndpointSourceError {
                        project,
                        source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            let validation = declaration.validate();
            if !validation.valid {
                source_errors.push(LocalEndpointSourceError {
                    project,
                    source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
                    error: validation.errors.join("; "),
                });
                continue;
            }

            for endpoint in declaration.endpoints {
                let observation = observation_cache
                    .entry((endpoint.scope.clone(), endpoint.port))
                    .or_insert_with(|| {
                        let addresses =
                            scope_addresses_with_tailnet(&endpoint.scope, endpoint.port, &tailnet);
                        probe_bound_addresses(&endpoint.scope, &addresses)
                    })
                    .clone();
                endpoints.push(CentralLocalEndpointReading {
                    project: project.clone(),
                    project_id: manifest.project_id.clone(),
                    source: PROJECT_LOCAL_ENDPOINTS_SOURCE.into(),
                    declaration: endpoint,
                    observation,
                    declared_conflict: false,
                });
            }
        }
    }

    let mut by_socket: BTreeMap<(String, String, u16), Vec<usize>> = BTreeMap::new();
    for (index, endpoint) in endpoints.iter().enumerate() {
        by_socket
            .entry((
                endpoint.declaration.protocol.clone(),
                endpoint.declaration.scope.clone(),
                endpoint.declaration.port,
            ))
            .or_default()
            .push(index);
    }

    let mut collisions = Vec::new();
    for ((protocol, scope, port), indexes) in by_socket {
        if indexes.len() < 2 {
            continue;
        }
        let mut owners = Vec::new();
        for index in indexes {
            endpoints[index].declared_conflict = true;
            owners.push(LocalEndpointOwner {
                project: endpoints[index].project.clone(),
                project_id: endpoints[index].project_id.clone(),
                endpoint_id: endpoints[index].declaration.id.clone(),
            });
        }
        collisions.push(DeclaredLocalEndpointCollision {
            protocol,
            scope,
            port,
            owners,
        });
    }

    endpoints.sort_by(|left, right| {
        left.declaration
            .port
            .cmp(&right.declaration.port)
            .then_with(|| left.project.cmp(&right.project))
            .then_with(|| left.declaration.id.cmp(&right.declaration.id))
    });

    Ok(CentralLocalEndpointRegistry {
        schema: CENTRAL_LOCAL_ENDPOINTS_SCHEMA.into(),
        generated_at_unix_seconds: now_unix_seconds(),
        endpoints,
        collisions,
        source_errors,
    })
}

pub fn refresh_central_local_endpoints(
    central_root: &Path,
) -> io::Result<(PathBuf, CentralLocalEndpointRegistry)> {
    let registry = inspect_central_local_endpoints(central_root)?;
    let path = central_root.join(CENTRAL_LOCAL_ENDPOINTS_CACHE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&registry)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    fs::write(&path, bytes)?;
    Ok((path, registry))
}

pub fn suggest_central_local_endpoint(
    central_root: &Path,
    range_start: u16,
    range_end: u16,
) -> io::Result<LocalEndpointSuggestion> {
    if range_start == 0 || range_start > range_end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "suggestion range must be within 1..=65535 and start <= end",
        ));
    }
    let span = u32::from(range_end) - u32::from(range_start) + 1;
    if span > MAX_SUGGEST_SPAN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("suggestion range may contain at most {MAX_SUGGEST_SPAN} ports"),
        ));
    }

    let registry = inspect_central_local_endpoints(central_root)?;
    let declared = registry
        .endpoints
        .iter()
        .map(|endpoint| endpoint.declaration.port)
        .collect::<BTreeSet<_>>();
    let tailnet = tailnet_interface_addresses();

    for port in range_start..=range_end {
        if declared.contains(&port) {
            continue;
        }
        // A wildcard bind is the cheapest honest rejection: it fails whenever
        // the port is held on any interface this machine exposes, so a port
        // occupied in any observable scope is never suggested.
        let wildcard = probe_bound_addresses(SCOPE_ANY, &wildcard_addresses(port));
        if wildcard.status != LocalEndpointOccupancy::Available {
            continue;
        }

        let mut free_in = Vec::new();
        let mut unverified_in = Vec::new();
        let mut blocked = false;
        for scope in LOCAL_ENDPOINT_SCOPES {
            let addresses = scope_addresses_with_tailnet(scope, port, &tailnet);
            if addresses.is_empty() {
                unverified_in.push(LocalEndpointScopeObservation {
                    scope: scope.to_owned(),
                    addresses: Vec::new(),
                    status: LocalEndpointOccupancy::Unknown,
                    detail: format!(
                        "{scope} scope has no observable interface address on this machine; no occupancy claim is made"
                    ),
                });
                continue;
            }
            let observation = probe_bound_addresses(scope, &addresses);
            match observation.status {
                LocalEndpointOccupancy::Available => {
                    free_in.push(LocalEndpointScopeObservation {
                        scope: scope.to_owned(),
                        addresses: addresses
                            .iter()
                            .map(|address| address.to_string())
                            .collect(),
                        status: observation.status,
                        detail: observation.detail,
                    });
                }
                // Occupied here would be a race with the wildcard probe, and
                // an unknown means the port cannot be verified; both skip.
                _ => {
                    blocked = true;
                    break;
                }
            }
        }
        if blocked {
            continue;
        }

        return Ok(LocalEndpointSuggestion {
            protocol: "tcp".into(),
            scope: SCOPE_LOCALHOST.into(),
            port,
            range_start,
            range_end,
            free_in,
            unverified_in,
        });
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "no undeclared and currently bindable TCP port found in {range_start}..={range_end} across the observable endpoint scopes"
        ),
    ))
}

struct LocalEndpointMutationLock {
    path: PathBuf,
}

impl Drop for LocalEndpointMutationLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn acquire_mutation_lock(central_root: &Path) -> io::Result<LocalEndpointMutationLock> {
    let lock_path = central_root.join(CENTRAL_LOCAL_ENDPOINTS_LOCK);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    for _ in 0..2 {
        match fs::create_dir(&lock_path) {
            Ok(()) => {
                let _ = fs::write(
                    lock_path.join("owner"),
                    format!(
                        "pid={}\ncreated_at={}\n",
                        std::process::id(),
                        now_unix_seconds()
                    ),
                );
                return Ok(LocalEndpointMutationLock { path: lock_path });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&lock_path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|elapsed| elapsed.as_secs() > LOCK_STALE_SECONDS);
                if stale {
                    let _ = fs::remove_dir_all(&lock_path);
                    continue;
                }
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another local-endpoint mutation is in progress; retry after it completes",
                ));
            }
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "local-endpoint mutation lock could not be acquired",
    ))
}

fn action_input(name: &str, input_type: &str, required: bool) -> ActionInputDefinition {
    ActionInputDefinition {
        name: name.into(),
        input_type: input_type.into(),
        required,
        choices: None,
        selection: None,
    }
}

fn descriptor(
    id: &str,
    title: &str,
    description: &str,
    mutation_class: MutationClass,
    output_type: &str,
    inputs: Vec<ActionInputDefinition>,
) -> ActionDescriptor {
    ActionDescriptor {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        inputs,
        output: ActionOutputDefinition {
            output_type: output_type.into(),
        },
        mutation_class,
        preview_supported: false,
        required_ports: Vec::new(),
        availability: ActionAvailability {
            available: true,
            reason: None,
        },
    }
}

fn required_text(input: &Value, field: &str, action: &str) -> Result<String, ActionResult> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ActionResult::failure(
                Some(action),
                ResultStatus::InvalidInput,
                format!("{action} requires {field}."),
                None,
            )
        })
}

fn optional_text(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn parse_port_value(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value != 0)
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.trim().parse::<u16>().ok())
                .filter(|value| *value != 0)
        })
}

fn required_port(input: &Value, field: &str, action: &str) -> Result<u16, ActionResult> {
    input.get(field).and_then(parse_port_value).ok_or_else(|| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("{action} requires {field} in 1..=65535."),
            None,
        )
    })
}

fn optional_port(input: &Value, field: &str) -> Option<u16> {
    input.get(field).and_then(parse_port_value)
}

fn optional_bool(input: &Value, field: &str) -> bool {
    input
        .get(field)
        .and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .as_str()
                    .and_then(|value| value.trim().parse::<bool>().ok())
            })
        })
        .unwrap_or(false)
}

fn ensure_project_member(raw: &str) -> io::Result<()> {
    let path = Path::new(raw);
    if raw.trim().is_empty()
        || raw != raw.trim()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project must be a safe Work-root-relative path",
        ));
    }
    Ok(())
}

fn project_context(
    action: &str,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> Result<(PathBuf, PathBuf, String), ActionResult> {
    let project = required_text(input, "project", action)?;
    ensure_project_member(&project).map_err(|error| {
        ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            error.to_string(),
            None,
        )
    })?;
    let root = resolve_central_root(context.root_options)
        .map_err(|message| {
            ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        })?
        .path;
    let project_root = root.join("Work").join(&project);
    if !project_root.is_dir() {
        return Err(ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!("Work project does not exist: {}", project_root.display()),
            None,
        ));
    }
    Ok((root, project_root, project))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        io::ErrorKind::WouldBlock => ResultStatus::UnavailableCapability,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn project_inspect_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.local-endpoints.inspect";
    let (_, project_root, _) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match inspect_project_local_endpoints(&project_root) {
        Ok(inspection) if inspection.valid => ActionResult::success(
            action,
            serde_json::to_value(inspection).expect("local endpoint inspection serializes"),
        ),
        Ok(inspection) => ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            "ProjectCentral local endpoint declaration is invalid.",
            Some(serde_json::to_value(inspection).expect("local endpoint inspection serializes")),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn project_set_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.local-endpoints.set";
    let (root, project_root, project) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let id = match required_text(input, "id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let kind = match required_text(input, "kind", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let port = match required_port(input, "port", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let scope = match optional_text(input, "scope") {
        Some(scope) => scope,
        None => SCOPE_LOCALHOST.to_owned(),
    };
    if !LOCAL_ENDPOINT_SCOPES.contains(&scope.as_str()) {
        return ActionResult::failure(
            Some(action),
            ResultStatus::InvalidInput,
            format!(
                "{action} requires scope to be one of {}.",
                LOCAL_ENDPOINT_SCOPES.join(", ")
            ),
            None,
        );
    }
    let allow_conflict = optional_bool(input, "allow_conflict");
    let mut endpoint = LocalEndpointDeclaration::localhost_tcp(id.clone(), kind, port);
    endpoint.scope = scope;
    endpoint.service = optional_text(input, "service");
    endpoint.description = optional_text(input, "description");

    let _lock = match acquire_mutation_lock(&root) {
        Ok(lock) => lock,
        Err(error) => return io_failure(action, error),
    };

    let registry = match inspect_central_local_endpoints(&root) {
        Ok(value) => value,
        Err(error) => return io_failure(action, error),
    };
    let conflicts = registry
        .endpoints
        .iter()
        .filter(|reading| {
            reading.declaration.protocol == endpoint.protocol
                && reading.declaration.scope == endpoint.scope
                && reading.declaration.port == endpoint.port
                && !(reading.project == project && reading.declaration.id == endpoint.id)
        })
        .map(|reading| LocalEndpointOwner {
            project: reading.project.clone(),
            project_id: reading.project_id.clone(),
            endpoint_id: reading.declaration.id.clone(),
        })
        .collect::<Vec<_>>();
    // The same port under a different scope is a coexistence, not a
    // collision; it stays visible in the success payload.
    let same_port_other_scopes = registry
        .endpoints
        .iter()
        .filter(|reading| {
            reading.declaration.protocol == endpoint.protocol
                && reading.declaration.port == endpoint.port
                && reading.declaration.scope != endpoint.scope
                && !(reading.project == project && reading.declaration.id == endpoint.id)
        })
        .map(|reading| {
            json!({
                "scope": reading.declaration.scope,
                "project": reading.project,
                "endpoint_id": reading.declaration.id,
            })
        })
        .collect::<Vec<_>>();
    if !conflicts.is_empty() && !allow_conflict {
        return ActionResult::failure(
            Some(action),
            ResultStatus::VerificationFailure,
            format!(
                "{} port {port} scoped {} is already declared; set allow_conflict=true only for intentional overlap",
                endpoint.protocol, endpoint.scope
            ),
            Some(json!({
                "protocol": endpoint.protocol,
                "scope": endpoint.scope,
                "port": port,
                "conflicts": conflicts
            })),
        );
    }

    match set_project_local_endpoint(&project_root, endpoint) {
        Ok(mutation) => match inspect_project_local_endpoints(&project_root) {
            Ok(inspection) => {
                let mut payload = json!({
                    "mutation": mutation,
                    "inspection": inspection,
                    "cross_project_conflicts": conflicts,
                });
                if !same_port_other_scopes.is_empty() {
                    payload["same_port_other_scopes"] = json!(same_port_other_scopes);
                    payload["same_port_other_scopes_note"] =
                        json!("the same protocol/port under a different scope is not a collision");
                }
                ActionResult::success(action, payload)
            }
            Err(error) => io_failure(action, error),
        },
        Err(error) => io_failure(action, error),
    }
}

fn project_remove_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "projectcentral.local-endpoints.remove";
    let (root, project_root, _) = match project_context(action, input, context) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let id = match required_text(input, "id", action) {
        Ok(value) => value,
        Err(result) => return result,
    };
    let _lock = match acquire_mutation_lock(&root) {
        Ok(lock) => lock,
        Err(error) => return io_failure(action, error),
    };
    match remove_project_local_endpoint(&project_root, &id) {
        Ok(mutation) => ActionResult::success(
            action,
            serde_json::to_value(mutation).expect("local endpoint mutation serializes"),
        ),
        Err(error) => io_failure(action, error),
    }
}

fn central_inspect_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.local-endpoints.inspect";
    let root = match resolve_central_root(context.root_options) {
        Ok(value) => value.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    inspect_central_local_endpoints(&root)
        .map(|registry| {
            ActionResult::success(
                action,
                serde_json::to_value(registry).expect("local endpoint registry serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn central_refresh_action(
    _: &ActionRegistry,
    _: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.local-endpoints.refresh";
    let root = match resolve_central_root(context.root_options) {
        Ok(value) => value.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    match refresh_central_local_endpoints(&root) {
        Ok((cache, registry)) => {
            ActionResult::success(action, json!({ "cache": cache, "registry": registry }))
        }
        Err(error) => io_failure(action, error),
    }
}

fn central_suggest_action(
    _: &ActionRegistry,
    input: &Value,
    context: &ActionExecutionContext<'_>,
) -> ActionResult {
    let action = "central.local-endpoints.suggest";
    let root = match resolve_central_root(context.root_options) {
        Ok(value) => value.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    let start = optional_port(input, "start").unwrap_or(DEFAULT_SUGGEST_START);
    let end = optional_port(input, "end").unwrap_or(DEFAULT_SUGGEST_END);
    suggest_central_local_endpoint(&root, start, end)
        .map(|suggestion| {
            ActionResult::success(
                action,
                serde_json::to_value(suggestion).expect("local endpoint suggestion serializes"),
            )
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

pub fn register_local_endpoint_actions(registry: &mut ActionRegistry) {
    let project = action_input("project", "string", true);
    let id = action_input("id", "string", true);
    let kind = action_input("kind", "string", true);
    let port = action_input("port", "integer", true);
    let mut scope = action_input("scope", "string", false);
    scope.choices = Some(
        LOCAL_ENDPOINT_SCOPES
            .iter()
            .map(|value| value.to_string())
            .collect(),
    );
    let service = action_input("service", "string", false);
    let description = action_input("description", "string", false);
    let allow_conflict = action_input("allow_conflict", "boolean", false);

    let actions = [
        (
            descriptor(
                "projectcentral.local-endpoints.inspect",
                "Inspect Project local endpoints",
                "Read one ProjectCentral endpoint declaration and report current machine occupancy for each declared scope (localhost, tailnet, or any) without mutating the declaration.",
                MutationClass::ReadOnly,
                "project-local-endpoint-inspection",
                vec![project.clone()],
            ),
            project_inspect_action
                as fn(&ActionRegistry, &Value, &ActionExecutionContext<'_>) -> ActionResult,
        ),
        (
            descriptor(
                "projectcentral.local-endpoints.set",
                "Set Project local endpoint",
                "Create or replace one Project-local TCP endpoint declaration scoped to localhost (default), the tailnet interfaces, or the wildcard binding, after re-checking the Central-wide allocation set.",
                MutationClass::LocallyMutating,
                "project-local-endpoint-mutation",
                vec![
                    project.clone(),
                    id.clone(),
                    kind,
                    port,
                    scope,
                    service,
                    description,
                    allow_conflict,
                ],
            ),
            project_set_action,
        ),
        (
            descriptor(
                "projectcentral.local-endpoints.remove",
                "Remove Project local endpoint",
                "Remove one Project-local localhost endpoint declaration by stable endpoint id.",
                MutationClass::LocallyMutating,
                "project-local-endpoint-mutation",
                vec![project, id],
            ),
            project_remove_action,
        ),
        (
            descriptor(
                "central.local-endpoints.inspect",
                "Inspect Central local endpoints",
                "Aggregate ProjectCentral endpoint declarations across Work, report declared collisions within the same scope, and observe current machine occupancy per scope.",
                MutationClass::ReadOnly,
                "central-local-endpoint-registry",
                vec![],
            ),
            central_inspect_action,
        ),
        (
            descriptor(
                "central.local-endpoints.refresh",
                "Refresh Central local endpoint snapshot",
                "Rebuild the Central-wide local endpoint reading and write it as derived .central state.",
                MutationClass::LocallyMutating,
                "central-local-endpoint-refresh",
                vec![],
            ),
            central_refresh_action,
        ),
        (
            descriptor(
                "central.local-endpoints.suggest",
                "Suggest available local endpoint",
                "Find the first TCP port in a bounded range that is neither declared by a Project nor occupied on any observable interface scope, and report the scopes where the port is free.",
                MutationClass::ReadOnly,
                "central-local-endpoint-suggestion",
                vec![
                    action_input("start", "integer", false),
                    action_input("end", "integer", false),
                ],
            ),
            central_suggest_action,
        ),
    ];

    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("local endpoint Action ids are valid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projectcentral::ProjectCentralManifest;
    use tempfile::tempdir;

    fn establish_project(central: &Path, name: &str, project_id: &str) -> PathBuf {
        let project = central.join("Work").join(name);
        fs::create_dir_all(project.join(PROJECTCENTRAL_DIR)).unwrap();
        let manifest = ProjectCentralManifest::new(project_id);
        let mut bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        bytes.push(b'\n');
        fs::write(
            project.join(PROJECTCENTRAL_DIR).join(PROJECT_MANIFEST),
            bytes,
        )
        .unwrap();
        project
    }

    #[test]
    fn declaration_validation_rejects_duplicate_project_socket() {
        let mut declaration = ProjectLocalEndpoints::default();
        declaration
            .endpoints
            .push(LocalEndpointDeclaration::localhost_tcp(
                "db", "database", 5432,
            ));
        declaration
            .endpoints
            .push(LocalEndpointDeclaration::localhost_tcp("api", "http", 5432));
        let validation = declaration.validate();
        assert!(!validation.valid);
        assert!(validation
            .errors
            .iter()
            .any(|error| error.contains("duplicate Project-local socket")));
    }

    #[test]
    fn set_update_and_remove_round_trip_project_source() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = establish_project(&central, "example", "example/project");

        let mut endpoint = LocalEndpointDeclaration::localhost_tcp("db", "database", 5432);
        endpoint.service = Some("postgres".into());
        let first = set_project_local_endpoint(&project, endpoint.clone()).unwrap();
        assert!(first.changed);
        assert!(project.join(PROJECT_LOCAL_ENDPOINTS_SOURCE).is_file());

        let same = set_project_local_endpoint(&project, endpoint.clone()).unwrap();
        assert!(!same.changed);

        endpoint.port = 55432;
        let update = set_project_local_endpoint(&project, endpoint).unwrap();
        assert!(update.changed);
        assert_eq!(
            read_project_local_endpoints(&project).unwrap().endpoints[0].port,
            55432
        );

        let removed = remove_project_local_endpoint(&project, "db").unwrap();
        assert!(removed.changed);
        assert!(read_project_local_endpoints(&project)
            .unwrap()
            .endpoints
            .is_empty());
    }

    #[test]
    fn central_registry_marks_cross_project_collisions() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let one = establish_project(&central, "one", "example/one");
        let two = establish_project(&central, "two", "example/two");
        set_project_local_endpoint(
            &one,
            LocalEndpointDeclaration::localhost_tcp("db", "database", 45432),
        )
        .unwrap();
        set_project_local_endpoint(
            &two,
            LocalEndpointDeclaration::localhost_tcp("api", "http", 45432),
        )
        .unwrap();

        let registry = inspect_central_local_endpoints(&central).unwrap();
        assert_eq!(registry.endpoints.len(), 2);
        assert_eq!(registry.collisions.len(), 1);
        assert!(registry
            .endpoints
            .iter()
            .all(|endpoint| endpoint.declared_conflict));
    }

    #[test]
    fn live_probe_distinguishes_an_occupied_tcp_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let observation = probe_local_tcp_port(port);
        assert_eq!(observation.status, LocalEndpointOccupancy::Occupied);
    }

    #[test]
    fn suggestion_never_returns_a_declared_port() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = establish_project(&central, "example", "example/project");

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let candidate = listener.local_addr().unwrap().port();
        drop(listener);
        set_project_local_endpoint(
            &project,
            LocalEndpointDeclaration::localhost_tcp("reserved", "http", candidate),
        )
        .unwrap();

        let result = suggest_central_local_endpoint(&central, candidate, candidate);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn v2_scopes_validate_and_unknown_scope_is_rejected() {
        let mut declaration = ProjectLocalEndpoints::default();
        declaration
            .endpoints
            .push(LocalEndpointDeclaration::localhost_tcp(
                "web", "http", 45140,
            ));
        let mut tailnet_endpoint =
            LocalEndpointDeclaration::localhost_tcp("stdb", "database", 45141);
        tailnet_endpoint.scope = SCOPE_TAILNET.into();
        let mut any_endpoint = LocalEndpointDeclaration::localhost_tcp("mesh", "http", 45142);
        any_endpoint.scope = SCOPE_ANY.into();
        declaration.endpoints.push(tailnet_endpoint);
        declaration.endpoints.push(any_endpoint);
        let validation = declaration.validate();
        assert!(validation.valid, "{:?}", validation.errors);

        let mut bogus = LocalEndpointDeclaration::localhost_tcp("bad", "http", 45143);
        bogus.scope = "multicast".into();
        declaration.endpoints.push(bogus);
        let validation = declaration.validate();
        assert!(!validation.valid);
        assert!(validation
            .errors
            .iter()
            .any(|error| error.contains("scope must be one of")));
    }

    #[test]
    fn v1_declaration_keeps_localhost_vocabulary() {
        let raw = r#"{"schema":"central.project.local-endpoints/v1","endpoints":[{"id":"db","kind":"database","scope":"tailnet","port":45150}]}"#;
        let parsed: ProjectLocalEndpoints = serde_json::from_str(raw).unwrap();
        let validation = parsed.validate();
        assert!(!validation.valid);
        assert!(validation
            .errors
            .iter()
            .any(|error| error.contains("must be localhost")));

        let raw = r#"{"schema":"central.project.local-endpoints/v1","endpoints":[{"id":"db","kind":"database","port":45151}]}"#;
        let parsed: ProjectLocalEndpoints = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.endpoints[0].scope, SCOPE_LOCALHOST);
        assert!(parsed.validate().valid);
    }

    #[test]
    fn v1_source_upgrades_to_v2_schema_when_mutated() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let project = establish_project(&central, "example", "example/project");
        let raw = r#"{"schema":"central.project.local-endpoints/v1","endpoints":[{"id":"db","kind":"database","port":45160}]}"#;
        fs::write(project.join(PROJECT_LOCAL_ENDPOINTS_SOURCE), raw).unwrap();

        set_project_local_endpoint(
            &project,
            LocalEndpointDeclaration::localhost_tcp("web", "http", 45161),
        )
        .unwrap();

        let written = fs::read_to_string(project.join(PROJECT_LOCAL_ENDPOINTS_SOURCE)).unwrap();
        assert!(written.contains(PROJECT_LOCAL_ENDPOINTS_SCHEMA));
        let reread = read_project_local_endpoints(&project).unwrap();
        assert_eq!(reread.endpoints.len(), 2);
        assert!(reread
            .endpoints
            .iter()
            .all(|endpoint| endpoint.scope == SCOPE_LOCALHOST));
    }

    #[test]
    fn cross_scope_declarations_are_not_collisions() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let one = establish_project(&central, "one", "example/one");
        let two = establish_project(&central, "two", "example/two");
        set_project_local_endpoint(
            &one,
            LocalEndpointDeclaration::localhost_tcp("web", "http", 45170),
        )
        .unwrap();
        let mut tailnet_endpoint =
            LocalEndpointDeclaration::localhost_tcp("stdb", "database", 45170);
        tailnet_endpoint.scope = SCOPE_TAILNET.into();
        set_project_local_endpoint(&two, tailnet_endpoint).unwrap();

        let registry = inspect_central_local_endpoints(&central).unwrap();
        assert_eq!(registry.endpoints.len(), 2);
        assert!(registry.collisions.is_empty());
        assert!(registry
            .endpoints
            .iter()
            .all(|endpoint| !endpoint.declared_conflict));
    }

    #[test]
    fn occupancy_follows_the_declaration_scope() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        let one = establish_project(&central, "one", "example/one");
        let two = establish_project(&central, "two", "example/two");
        let three = establish_project(&central, "three", "example/three");

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        set_project_local_endpoint(
            &one,
            LocalEndpointDeclaration::localhost_tcp("web", "http", port),
        )
        .unwrap();
        let mut tailnet_endpoint =
            LocalEndpointDeclaration::localhost_tcp("stdb", "database", port);
        tailnet_endpoint.scope = SCOPE_TAILNET.into();
        set_project_local_endpoint(&two, tailnet_endpoint).unwrap();
        let mut any_endpoint = LocalEndpointDeclaration::localhost_tcp("mesh", "http", port);
        any_endpoint.scope = SCOPE_ANY.into();
        set_project_local_endpoint(&three, any_endpoint).unwrap();

        let registry = inspect_central_local_endpoints(&central).unwrap();
        assert_eq!(registry.endpoints.len(), 3);
        for endpoint in &registry.endpoints {
            match endpoint.declaration.scope.as_str() {
                SCOPE_LOCALHOST => assert_eq!(
                    endpoint.observation.status,
                    LocalEndpointOccupancy::Occupied
                ),
                SCOPE_TAILNET => {
                    // Free where a tailnet interface exists and probed, honest
                    // `unknown` where the machine has no tailnet address.
                    assert_ne!(
                        endpoint.observation.status,
                        LocalEndpointOccupancy::Occupied
                    );
                }
                SCOPE_ANY => assert_eq!(
                    endpoint.observation.status,
                    LocalEndpointOccupancy::Occupied
                ),
                other => panic!("unexpected scope {other}"),
            }
        }
    }

    #[test]
    fn tailnet_discovery_and_range_checks() {
        assert!(discover_interface_addresses().contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(is_tailnet_address("100.92.62.101".parse().unwrap()));
        assert!(is_tailnet_address("100.64.0.1".parse().unwrap()));
        assert!(is_tailnet_address("100.127.255.254".parse().unwrap()));
        assert!(!is_tailnet_address("100.63.0.1".parse().unwrap()));
        assert!(!is_tailnet_address("100.128.0.1".parse().unwrap()));
        assert!(!is_tailnet_address("192.168.4.90".parse().unwrap()));
        assert!(is_tailnet_address("fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(!is_tailnet_address("fd7a:115c:a1e1::1".parse().unwrap()));
        assert!(!is_tailnet_address("fe80::1".parse().unwrap()));
        for address in tailnet_interface_addresses() {
            assert!(is_tailnet_address(address));
        }
    }

    #[test]
    fn suggest_skips_ports_occupied_on_any_interface() {
        let temp = tempdir().unwrap();
        let central = temp.path().join("Central");
        establish_project(&central, "example", "example/project");

        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let candidate = listener.local_addr().unwrap().port();
        // The port is free on loopback-adjacent scopes the moment the
        // wildcard listener disappears, but while it is held on the wildcard
        // binding it is occupied on every interface and must not be offered.
        let result = suggest_central_local_endpoint(&central, candidate, candidate);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        drop(listener);

        let suggestion = suggest_central_local_endpoint(&central, candidate, candidate).unwrap();
        assert_eq!(suggestion.port, candidate);
        assert_eq!(suggestion.scope, SCOPE_LOCALHOST);
        assert!(suggestion
            .free_in
            .iter()
            .all(|scope| scope.status == LocalEndpointOccupancy::Available));
        let free_scopes = suggestion
            .free_in
            .iter()
            .map(|scope| scope.scope.as_str())
            .collect::<Vec<_>>();
        assert!(free_scopes.contains(&SCOPE_LOCALHOST));
        assert!(free_scopes.contains(&SCOPE_ANY));
        assert!(suggestion
            .unverified_in
            .iter()
            .all(|scope| scope.status == LocalEndpointOccupancy::Unknown));
    }
}
