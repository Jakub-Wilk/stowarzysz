//! Generates the API types from the committed `OpenAPI` snapshot (`openapi.json`).

use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=openapi.json");
    let text = std::fs::read_to_string("openapi.json").expect("openapi.json is readable");
    // OpenAPI 3.1 components are JSON Schema; point the refs at `definitions` for typify.
    let text = text.replace("#/components/schemas/", "#/definitions/");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("openapi.json is JSON");
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": doc["components"]["schemas"],
    });
    let schema: schemars::schema::RootSchema =
        serde_json::from_value(schema).expect("the components are JSON Schema");
    let mut space = typify::TypeSpace::new(
        typify::TypeSpaceSettings::default()
            .with_struct_builder(false)
            // Dioxus compares component props, so every type needs PartialEq.
            .with_derive("PartialEq".to_owned()),
    );
    space
        .add_root_schema(schema)
        .expect("typify understands the schema");
    let file = syn::parse2::<syn::File>(space.to_stream()).expect("typify emits Rust");
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR is set")).join("types.rs");
    std::fs::write(out, prettyplease::unparse(&file)).expect("OUT_DIR is writable");
}
