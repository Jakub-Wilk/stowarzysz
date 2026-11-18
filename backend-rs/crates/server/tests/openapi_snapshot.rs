//! frontend-rs generates its API types from a committed copy of the OpenAPI document; a
//! contract change must update it in the same commit (`frontend-rs/scripts/update-openapi.sh`).

use std::path::Path;

#[test]
fn openapi_snapshot_is_current() {
    let current = server::http::api()
        .split_for_parts()
        .1
        .to_pretty_json()
        .expect("the OpenAPI document serialises");
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../frontend-rs/crates/api/openapi.json");
    let Ok(snapshot) = std::fs::read_to_string(&path) else {
        eprintln!("no frontend-rs checkout next to backend-rs; skipping");
        return;
    };
    assert!(
        snapshot.replace("\r\n", "\n").trim_end() == current.trim_end(),
        "the API contract changed: run frontend-rs/scripts/update-openapi.sh and commit openapi.json"
    );
}
