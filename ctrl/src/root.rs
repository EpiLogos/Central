use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const REQUIRED_DIRECTORIES: [&str; 5] = [
    "Control/user",
    "Control/agents",
    "Control/machines",
    ".central",
    "Work",
];

#[derive(Debug, Clone, Default)]
pub struct RootOptions {
    pub explicit_root: Option<PathBuf>,
    pub configured_root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootSource {
    Explicit,
    Environment,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedRoot {
    pub path: PathBuf,
    pub source: RootSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectoryCheck {
    pub path: String,
    pub valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MixedRootSignal {
    /// A root-level `Cargo.toml` (generic by itself; meaningful combined with other signals).
    CargoManifest,
    /// A root-level `ctrl/` source directory — specific to the Central product.
    CtrlSourceDirectory,
    /// A `crates/connector-sdk` crate — specific to the Central product.
    ConnectorSdkCrate,
    /// A root-level `connectors/` directory (generic by itself; meaningful combined with the product remote).
    ConnectorsDirectory,
    /// A git remote whose URL points at `EpiLogos/Central`.
    ProductRepositoryRemote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MixedRootDiagnostic {
    pub detected: bool,
    pub signals: Vec<MixedRootSignal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Strong-signal detection for a personal Central root that is also the Central product
/// source checkout. Single generic files (a lone `Cargo.toml`, a lone `connectors/` dir)
/// never trigger it; the collision requires a product-specific combination.
fn detect_mixed_root(root: &Path) -> MixedRootDiagnostic {
    let has_cargo = fs::metadata(root.join("Cargo.toml"))
        .map(|metadata| metadata.is_file())
        .unwrap_or(false);
    let has_ctrl_dir = fs::metadata(root.join("ctrl"))
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    let has_sdk = fs::metadata(root.join("crates/connector-sdk"))
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    let has_connectors = fs::metadata(root.join("connectors"))
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);

    let mut signals = Vec::new();
    if has_cargo {
        signals.push(MixedRootSignal::CargoManifest);
    }
    if has_ctrl_dir {
        signals.push(MixedRootSignal::CtrlSourceDirectory);
    }
    if has_sdk {
        signals.push(MixedRootSignal::ConnectorSdkCrate);
    }
    if has_connectors {
        signals.push(MixedRootSignal::ConnectorsDirectory);
    }
    if product_remote_present(root) {
        signals.push(MixedRootSignal::ProductRepositoryRemote);
    }

    let detected =
        (has_cargo && has_ctrl_dir) || has_sdk || (has_connectors && product_remote_present(root));

    let message = if detected {
        Some(
            "Central personal root is also the Central product source checkout. \
             Control/ and Work/ are the personal authored world; product source should live \
             in a developer checkout elsewhere (for example under Work/ or another source root)."
                .to_owned(),
        )
    } else {
        None
    };

    MixedRootDiagnostic {
        detected,
        signals,
        message,
    }
}

const PRODUCT_REMOTE_PATTERNS: [&str; 2] = [
    "https://github.com/EpiLogos/Central",
    "git@github.com:EpiLogos/Central",
];

fn product_remote_present(root: &Path) -> bool {
    let dot_git = root.join(".git");
    let config = if dot_git.is_dir() {
        dot_git.join("config")
    } else if dot_git.is_file() {
        // A worktree gitdir pointer: resolve the real gitdir, then read its config.
        let pointer = match fs::read_to_string(&dot_git) {
            Ok(text) => text,
            Err(_) => return false,
        };
        let Some(gitdir) = pointer
            .lines()
            .find_map(|line| line.strip_prefix("gitdir: "))
        else {
            return false;
        };
        let gitdir = PathBuf::from(gitdir);
        let gitdir = if gitdir.is_absolute() {
            gitdir
        } else {
            root.join(gitdir)
        };
        gitdir.join("config")
    } else {
        return false;
    };
    let Ok(text) = fs::read_to_string(config) else {
        return false;
    };
    PRODUCT_REMOTE_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralHealth {
    pub root: PathBuf,
    pub root_state: String,
    pub valid: bool,
    pub checks: Vec<DirectoryCheck>,
    pub mixed_root: MixedRootDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CentralInitialization {
    pub root: PathBuf,
    pub directories: Vec<String>,
}

pub fn resolve_central_root(options: &RootOptions) -> Result<ResolvedRoot, String> {
    if let Some(path) = options.explicit_root.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: path.clone(), source: RootSource::Explicit });
    }
    if let Some(path) = options.configured_root.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: path.clone(), source: RootSource::Environment });
    }
    if let Some(home) = options.home.as_ref().filter(|path| !path.as_os_str().is_empty()) {
        return Ok(ResolvedRoot { path: home.join("Central"), source: RootSource::Default });
    }
    Err("Central root cannot be resolved because no home directory is available.".to_owned())
}

pub fn inspect_central(root: &Path) -> io::Result<CentralHealth> {
    let root_state = match fs::metadata(root) {
        Ok(metadata) if metadata.is_dir() => "directory",
        Ok(_) => "not_directory",
        Err(error) if error.kind() == io::ErrorKind::NotFound => "missing",
        Err(error) => return Err(error),
    };

    let checks = REQUIRED_DIRECTORIES
        .iter()
        .map(|relative| DirectoryCheck {
            path: (*relative).to_owned(),
            valid: fs::metadata(root.join(relative)).map(|metadata| metadata.is_dir()).unwrap_or(false),
        })
        .collect::<Vec<_>>();
    let valid = root_state == "directory" && checks.iter().all(|check| check.valid);

    Ok(CentralHealth {
        root: root.to_path_buf(),
        root_state: root_state.to_owned(),
        valid,
        checks,
        mixed_root: detect_mixed_root(root),
    })
}

pub fn initialize_central(root: &Path) -> io::Result<CentralInitialization> {
    if let Ok(metadata) = fs::metadata(root) {
        if !metadata.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Central root exists but is not a directory."));
        }
    }
    fs::create_dir_all(root)?;
    for relative in REQUIRED_DIRECTORIES {
        fs::create_dir_all(root.join(relative))?;
    }
    Ok(CentralInitialization {
        root: root.to_path_buf(),
        directories: REQUIRED_DIRECTORIES.iter().map(|item| (*item).to_owned()).collect(),
    })
}
