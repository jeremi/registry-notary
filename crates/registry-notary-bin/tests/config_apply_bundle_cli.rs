// SPDX-License-Identifier: Apache-2.0
//! Binary-level coverage for governed configuration bundle apply.

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn apply_bundle_command(server: &MockServer) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_registry-notary"));
    command
        .arg("config")
        .arg("apply-bundle")
        .arg("--admin-url")
        .arg(server.uri())
        .arg("--admin-token-env")
        .arg("NOTARY_ADMIN_TOKEN_TEST")
        .arg("--root-path")
        .arg("/etc/registry-notary/tuf/metadata/1.root.json")
        .arg("--metadata-dir")
        .arg("/etc/registry-notary/tuf/metadata")
        .arg("--targets-dir")
        .arg("/etc/registry-notary/tuf/targets")
        .arg("--datastore-dir")
        .arg("/var/lib/registry-notary/tuf")
        .arg("--target-name")
        .arg("registry-notary.yaml")
        .arg("--local-approval-reference")
        .arg("ROOT-2026-Q2")
        .env("NOTARY_ADMIN_TOKEN_TEST", "operator-token")
        .env_remove("REGISTRY_NOTARY_CONFIG");
    command
}

#[tokio::test]
async fn config_apply_bundle_cli_posts_local_tuf_request_to_admin_apply() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/v1/config/apply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": "accepted",
            "bundle_id": "notary-test-bundle"
        })))
        .mount(&server)
        .await;

    let output = apply_bundle_command(&server)
        .output()
        .expect("apply-bundle command runs");

    assert!(
        output.status.success(),
        "apply-bundle failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("apply-bundle emits server JSON");
    assert_eq!(response["result"], "accepted");
    assert_eq!(response["bundle_id"], "notary-test-bundle");

    let requests = server
        .received_requests()
        .await
        .expect("request recording is enabled");
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url.path(), "/admin/v1/config/apply");
    assert_eq!(
        request
            .headers
            .get("authorization")
            .expect("authorization header is present")
            .to_str()
            .expect("authorization header is valid"),
        "Bearer operator-token"
    );
    let body: Value = request.body_json().expect("request body is JSON");
    assert_eq!(
        body,
        json!({
            "tuf": {
                "root_path": path_string("/etc/registry-notary/tuf/metadata/1.root.json"),
                "metadata_dir": path_string("/etc/registry-notary/tuf/metadata"),
                "targets_dir": path_string("/etc/registry-notary/tuf/targets"),
                "datastore_dir": path_string("/var/lib/registry-notary/tuf"),
                "target_name": "registry-notary.yaml"
            },
            "local_approval_reference": "ROOT-2026-Q2"
        })
    );
}

#[tokio::test]
async fn config_apply_bundle_cli_exits_nonzero_on_admin_apply_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/v1/config/apply"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "result": "restart_required",
            "detail": "candidate requires restart"
        })))
        .mount(&server)
        .await;

    let output = apply_bundle_command(&server)
        .output()
        .expect("apply-bundle command runs");

    assert!(
        !output.status.success(),
        "apply-bundle unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("apply-bundle emits server JSON");
    assert_eq!(response["result"], "restart_required");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("admin config apply returned HTTP 409"),
        "stderr did not report non-2xx status:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_string(path: &str) -> String {
    Path::new(path).to_string_lossy().into_owned()
}
