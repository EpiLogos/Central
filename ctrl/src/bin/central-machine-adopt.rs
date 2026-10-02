//! Compatibility CLI: adoption authority lives exclusively in machine.adopt-current.
use central_ctrl::{
    create_core_action_registry, create_default_connector_registry, ActionExecutionContext,
    ActionResult, ConnectorContext, MachineDeclaration, ResultStatus, RootOptions,
};
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf, process::ExitCode};

const DEFAULT_ROLE: &str = "current";
const DEFAULT_WORKCELL_REF: &str = "workcell:local";

#[derive(Debug)]
struct Args {
    root: PathBuf,
    role: String,
    workcell_ref: String,
    json: bool,
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("central-machine-adopt: {message}");
            return ExitCode::from(2);
        }
    };
    let json = args.json;
    match run(args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(result) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&result).expect("native ActionResult serializes")
                );
            } else if let Some(error) = &result.error {
                eprintln!("central-machine-adopt: {}: {}", error.code, error.message);
            }
            ExitCode::from(2)
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut root = None;
    let mut role = DEFAULT_ROLE.to_owned();
    let mut workcell_ref = DEFAULT_WORKCELL_REF.to_owned();
    let mut json = false;
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--root requires a path".to_owned())?,
                ));
            }
            "--role" => {
                index += 1;
                role = args
                    .get(index)
                    .ok_or_else(|| "--role requires a value".to_owned())?
                    .to_owned();
            }
            "--workcell-ref" => {
                index += 1;
                workcell_ref = args
                    .get(index)
                    .ok_or_else(|| "--workcell-ref requires a value".to_owned())?
                    .to_owned();
            }
            "--json" => json = true,
            "--help" | "-h" => {
                return Err(
                    "usage: central-machine-adopt --root PATH [--role current] [--workcell-ref workcell:local] [--json]"
                        .to_owned(),
                );
            }
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }

    let root = root.ok_or_else(|| "--root is required".to_owned())?;
    if workcell_ref.trim().is_empty() {
        return Err("--workcell-ref must be non-empty".to_owned());
    }

    Ok(Args {
        root,
        role,
        workcell_ref,
        json,
    })
}

fn run(args: Args) -> Result<String, Box<ActionResult>> {
    let registry = create_core_action_registry();
    let connectors = create_default_connector_registry();
    let connector_context = ConnectorContext::current();
    let root_options = RootOptions {
        explicit_root: Some(args.root.clone()),
        configured_root: None,
        home: None,
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };
    let result = registry.execute(
        "machine.adopt-current",
        &json!({
            "role": args.role, "workcell_ref": args.workcell_ref,
        }),
        &context,
    );
    if !result.ok {
        return Err(Box::new(result));
    }
    #[cfg(test)]
    tests::owner_returned(&result);
    let projection_failure = |message: String, path: Option<&std::path::Path>| {
        Box::new(ActionResult::failure_coded(
            Some("machine.adopt-current"),
            ResultStatus::PartialCompletion,
            "machine.compatibility_readback_failed",
            message,
            Some(
                json!({"owner_completed":true,"compatibility_readback_confirmed":false,
                "native_result":&result,"source_path":path,"automatic_retry":false}),
            ),
        ))
    };
    let mut data = result.data.clone().ok_or_else(|| {
        projection_failure("machine.adopt-current returned no result".to_owned(), None)
    })?;
    let relative = data["source"]["path"].as_str().ok_or_else(|| {
        projection_failure(
            "machine.adopt-current returned no source path".to_owned(),
            None,
        )
    })?;
    let path = args.root.join(relative);
    let native_declaration: MachineDeclaration =
        serde_json::from_value(data["declaration"].clone()).map_err(|error| {
            projection_failure(
                format!(
                "Native adoption succeeded but its declaration receipt cannot be decoded: {error}"
            ),
                Some(&path),
            )
        })?;
    // This is a read-only compatibility projection of the successful owner's
    // source. The native typed declaration intentionally omits extension fields;
    // the historical CLI result includes them, without acquiring mutation authority.
    let raw: Value = serde_json::from_slice(&fs::read(&path).map_err(|error|
        projection_failure(format!("Native adoption succeeded but compatibility readback failed: {error}; inspect {} before retrying", path.display()), Some(&path)))?)
        .map_err(|error| projection_failure(format!("Native adoption succeeded but compatibility readback is invalid: {error}; inspect {} before retrying", path.display()), Some(&path)))?;
    let reading: MachineDeclaration = serde_json::from_value(raw.clone()).map_err(|error| {
        projection_failure(
            format!("Native adoption succeeded but compatibility readback is not a machine declaration: {error}; inspect {} before retrying", path.display()),
            Some(&path),
        )
    })?;
    if reading != native_declaration {
        return Err(projection_failure(
            format!("Native adoption succeeded but compatibility readback does not match the accepted native declaration; inspect {} before retrying", path.display()),
            Some(&path),
        ));
    }
    let relative = relative.to_owned();
    data["declaration"] = raw;
    data["ok"] = Value::Bool(true);
    if args.json {
        serde_json::to_string_pretty(&data)
            .map_err(|error| projection_failure(error.to_string(), Some(&path)))
    } else {
        Ok(format!(
            "{}: {} -> {} ({})",
            data["outcome"].as_str().unwrap_or("unknown"),
            data["role"].as_str().unwrap_or(DEFAULT_ROLE),
            data["workcell_ref"]
                .as_str()
                .unwrap_or(DEFAULT_WORKCELL_REF),
            relative
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NativeFixture(PathBuf);
    impl NativeFixture {
        fn new_in(parent: &std::path::Path) -> Self {
            let path = parent.join(format!("native-source-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for NativeFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    std::thread_local! {
        static AFTER_OWNER_RETURN: std::cell::RefCell<Option<Box<dyn FnOnce(&ActionResult)>>> =
            const { std::cell::RefCell::new(None) };
    }
    pub(super) fn owner_returned(result: &ActionResult) {
        AFTER_OWNER_RETURN.with(|slot| {
            if let Some(observer) = slot.borrow_mut().take() {
                observer(result);
            }
        });
    }
    #[test]
    fn actual_native_adoption_success_survives_failed_compatibility_reading() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let root = fixture.path().join("world");
        central_ctrl::initialize_central(&root).unwrap();
        let retained = fixture.path().join("retained-authored-machine.json");
        let retained_for_observer = retained.clone();
        let observed_root = root.clone();
        AFTER_OWNER_RETURN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |result| {
                assert!(
                    result.ok,
                    "the actual native owner must complete before the adverse reading"
                );
                let relative = result.data.as_ref().unwrap()["source"]["path"]
                    .as_str()
                    .unwrap();
                fs::rename(observed_root.join(relative), &retained_for_observer).unwrap();
            }));
        });
        let failure = run(Args {
            root,
            role: "current".to_owned(),
            workcell_ref: "workcell:compatibility-proof".to_owned(),
            json: true,
        })
        .unwrap_err();
        let value = serde_json::to_value(&failure).unwrap();
        assert_eq!(value["status"], "partial_completion");
        assert_eq!(
            value["error"]["code"],
            "machine.compatibility_readback_failed"
        );
        assert_eq!(value["error"]["details"]["owner_completed"], true);
        assert_eq!(
            value["error"]["details"]["compatibility_readback_confirmed"],
            false
        );
        assert_eq!(value["error"]["details"]["native_result"]["ok"], true);
        assert_eq!(
            value["error"]["details"]["native_result"]["data"]["outcome"],
            "created"
        );
        assert!(value["error"]["details"].get("operation_ref").is_none());
        let actual: Value = serde_json::from_slice(&fs::read(retained).unwrap()).unwrap();
        assert_eq!(
            actual["bindings"],
            json!([{"kind":"workcell","reference":"workcell:compatibility-proof"}])
        );
    }

    #[test]
    fn actual_native_adoption_receipt_survives_changed_or_non_declaration_readback() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        for changed_binding in [true, false] {
            let fixture = NativeFixture::new_in(&scratch);
            let root = fixture.path().join("world");
            central_ctrl::initialize_central(&root).unwrap();
            let observed_root = root.clone();
            AFTER_OWNER_RETURN.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move |result| {
                    assert!(result.ok, "the real native owner must complete first");
                    let data = result.data.as_ref().unwrap();
                    let path = observed_root.join(data["source"]["path"].as_str().unwrap());
                    let raw = if changed_binding {
                        let mut raw: Value =
                            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                        raw["bindings"][0]["reference"] =
                            json!("workcell:concurrent-foreign-binding");
                        raw
                    } else {
                        json!({"foreign_document":true})
                    };
                    fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
                }));
            });
            let failure = run(Args {
                root: root.clone(),
                role: "current".to_owned(),
                workcell_ref: "workcell:compatibility-proof".to_owned(),
                json: true,
            })
            .unwrap_err();
            let value = serde_json::to_value(&failure).unwrap();
            let details = &value["error"]["details"];
            assert_eq!(value["status"], "partial_completion");
            assert_eq!(
                value["error"]["code"],
                "machine.compatibility_readback_failed"
            );
            assert_eq!(details["owner_completed"], true);
            assert_eq!(details["compatibility_readback_confirmed"], false);
            assert_eq!(details["automatic_retry"], false);
            assert_eq!(details["native_result"]["ok"], true);
            assert_eq!(details["native_result"]["data"]["outcome"], "created");
            assert_eq!(
                details["native_result"]["data"]["declaration"]["bindings"],
                json!([{"kind":"workcell","reference":"workcell:compatibility-proof"}])
            );
            let actual_path = root.join(
                details["native_result"]["data"]["source"]["path"]
                    .as_str()
                    .unwrap(),
            );
            assert_eq!(details["source_path"], json!(actual_path));
            assert!(details.get("operation_ref").is_none());
            let current: Value = serde_json::from_slice(&fs::read(actual_path).unwrap()).unwrap();
            if changed_binding {
                assert_eq!(
                    current["bindings"][0]["reference"],
                    "workcell:concurrent-foreign-binding"
                );
                assert!(value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("does not match"));
            } else {
                assert_eq!(current, json!({"foreign_document":true}));
                assert!(value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("not a machine declaration"));
            }
        }
    }

    #[test]
    fn actual_matching_native_declaration_keeps_unknown_compatibility_extensions() {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let fixture = NativeFixture::new_in(&scratch);
        let root = fixture.path().join("world");
        central_ctrl::initialize_central(&root).unwrap();
        let observed_root = root.clone();
        AFTER_OWNER_RETURN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |result| {
                assert!(result.ok, "the real native owner must complete first");
                let data = result.data.as_ref().unwrap();
                let path = observed_root.join(data["source"]["path"].as_str().unwrap());
                let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                raw["operator_extension"] = json!({"retained_exactly":[1,"source",true]});
                let reading: MachineDeclaration = serde_json::from_value(raw.clone()).unwrap();
                let native: MachineDeclaration =
                    serde_json::from_value(data["declaration"].clone()).unwrap();
                assert_eq!(reading, native);
                fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
            }));
        });
        let output = run(Args {
            root,
            role: "current".to_owned(),
            workcell_ref: "workcell:compatibility-proof".to_owned(),
            json: true,
        })
        .unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["outcome"], "created");
        assert_eq!(
            value["declaration"]["bindings"],
            json!([{"kind":"workcell","reference":"workcell:compatibility-proof"}])
        );
        assert_eq!(
            value["declaration"]["operator_extension"],
            json!({"retained_exactly":[1,"source",true]})
        );
    }
}
