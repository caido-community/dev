use super::{package_plan, BuildOutput, PackageConfig};
use serde_json::{json, Value};

fn config() -> Value {
    json!({"id":"package","plugins":[{"id":"plugin","root":"plugin"}]})
}

fn output() -> Value {
    json!({"kind":"frontend","id":"plugin","fileName":"plugin/index.js","assets":[]})
}

#[test]
fn config_ids_and_references_are_checked_at_deserialization() {
    for id in ["../escape", "a/b", "a\\b", "CON", "", "a\n"] {
        let mut value = config();
        value["id"] = json!(id);
        assert!(serde_json::from_value::<PackageConfig>(value).is_err());
        let mut value = config();
        value["plugins"][0]["id"] = json!(id);
        assert!(serde_json::from_value::<PackageConfig>(value).is_err());
        let mut value = config();
        value["plugins"][0]["backend"] = json!({"id":id});
        assert!(serde_json::from_value::<PackageConfig>(value).is_err());
    }
    let mut value = config();
    value["plugins"] = json!([{"id":"plugin","root":"a"},{"id":"PLUGIN","root":"b"}]);
    assert!(serde_json::from_value::<PackageConfig>(value)
        .unwrap_err()
        .to_string()
        .contains("Duplicate plugin id"));
}

#[test]
fn output_ids_shapes_and_references_are_checked_at_deserialization() {
    for key in ["id", "backendId"] {
        let mut value = output();
        value[key] = json!("../escape");
        assert!(serde_json::from_value::<BuildOutput>(value).is_err());
    }
    for (key, invalid) in [
        ("kind", json!("workflow")),
        ("fileName", json!(42)),
        ("assets", json!([false])),
    ] {
        let mut value = output();
        value[key] = invalid;
        assert!(serde_json::from_value::<BuildOutput>(value).is_err());
    }
}

#[test]
fn duplicate_output_ids_and_build_asset_directory_collisions_are_rejected() {
    let config = || serde_json::from_value::<PackageConfig>(config()).unwrap();
    let output = || serde_json::from_value::<BuildOutput>(output()).unwrap();
    assert!(package_plan(config(), vec![output(), output()]).is_err());
    let mut collision = output();
    collision.file_name = "plugin/assets".into();
    collision.assets.push("assets/*".into());
    assert!(package_plan(config(), vec![collision]).is_err());
}

#[test]
fn missing_null_and_empty_output_names_remain_distinct() {
    for name in [None, Some(Value::Null), Some(json!(""))] {
        let mut value = output();
        if let Some(name) = &name {
            value["name"] = name.clone();
        }
        let plan = package_plan(
            serde_json::from_value(config()).unwrap(),
            vec![serde_json::from_value(value).unwrap()],
        )
        .unwrap();
        let manifest = serde_json::to_value(plan.manifest).unwrap();
        assert_eq!(
            manifest["plugins"][0]["name"],
            if name == Some(json!("")) {
                ""
            } else {
                "plugin"
            }
        );
    }
}

#[test]
fn empty_optional_frontend_fields_do_not_generate_paths_or_references() {
    let mut value = output();
    value["cssFileName"] = json!("");
    value["backendId"] = json!("");
    let plan = package_plan(
        serde_json::from_value(config()).unwrap(),
        vec![serde_json::from_value(value).unwrap()],
    )
    .unwrap();
    let manifest = serde_json::to_value(plan.manifest).unwrap();
    assert!(manifest["plugins"][0].get("style").is_none());
    assert!(manifest["plugins"][0]["backend"].is_null());
}
