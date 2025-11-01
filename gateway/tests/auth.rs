#![allow(clippy::print_stdout)]
use std::process::Stdio;
use std::str::FromStr;

use http::{Method, StatusCode};
use serde_json::json;
use tensorzero::test_helpers::make_embedded_gateway_with_config_and_postgres;
use tensorzero_auth::key::TensorZeroApiKey;
use tensorzero_core::endpoints::status::TENSORZERO_VERSION;
use tokio::process::Command;

use crate::common::start_gateway_on_random_port;
use secrecy::ExposeSecret;

mod common;

const GATEWAY_PATH: &str = env!("CARGO_BIN_EXE_gateway");

#[tokio::test]
async fn test_tensorzero_auth_enabled() {
    let child_data = start_gateway_on_random_port(
        "
    [gateway.auth]
    enabled = true
    ",
        None,
    )
    .await;

    let embedded_client = make_embedded_gateway_with_config_and_postgres(
        "
    [gateway.auth]
    enabled = true
    ",
    )
    .await;

    let postgres_pool = embedded_client
        .get_app_state_data()
        .unwrap()
        .postgres_connection_info
        .get_alpha_pool()
        .unwrap();

    let key = tensorzero_auth::postgres::create_key("my_org", "my_workspace", None, postgres_pool)
        .await
        .unwrap();

    let inference_response = reqwest::Client::new()
        .post(format!("http://{}/inference", child_data.addr))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {}", key.expose_secret()),
        )
        .json(&json!({
            "model_name": "dummy::good",
            "input": {
                "messages": [
                    {
                        "role": "user",
                        "content": "Hello, world!",
                    }
                ]
            }
        }))
        .send()
        .await
        .unwrap();

    let status = inference_response.status();
    let text = inference_response.text().await.unwrap();
    println!("API response: {text}");
    assert_eq!(status, StatusCode::OK);

    // The key should stop working after we disable it
    let disabled_at = tensorzero_auth::postgres::disable_key(
        &TensorZeroApiKey::parse(key.expose_secret())
            .unwrap()
            .public_id,
        postgres_pool,
    )
    .await
    .unwrap();

    let inference_response = reqwest::Client::new()
        .post(format!("http://{}/inference", child_data.addr))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {}", key.expose_secret()),
        )
        .json(&json!({
            "model_name": "dummy::good",
            "input": {
                "messages": [
                    {
                        "role": "user",
                        "content": "Hello, world!",
                    }
                ]
            }
        }))
        .send()
        .await
        .unwrap();

    let status = inference_response.status();
    let text = inference_response.text().await.unwrap();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(text, format!("{{\"error\":\"TensorZero authentication error: API key was disabled at: {disabled_at}\"}}"));
}

#[tokio::test]
async fn test_tensorzero_unauthenticated_routes() {
    // The /health and /status routes should always be unauthenticated
    let child_data = start_gateway_on_random_port(
        "
    [gateway.auth]
    enabled = true
    ",
        None,
    )
    .await;

    let health_response = reqwest::Client::new()
        .request(Method::GET, format!("http://{}/health", child_data.addr))
        .send()
        .await
        .unwrap();

    let status = health_response.status();
    let text = health_response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        text,
        "{\"gateway\":\"ok\",\"clickhouse\":\"ok\",\"postgres\":\"ok\"}"
    );

    let status_response = reqwest::Client::new()
        .request(Method::GET, format!("http://{}/status", child_data.addr))
        .send()
        .await
        .unwrap();

    let status = status_response.status();
    let text = status_response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        text,
        format!("{{\"status\":\"ok\",\"version\":\"{TENSORZERO_VERSION}\"}}")
    );
}

#[tokio::test]
async fn test_tensorzero_missing_auth() {
    let child_data = start_gateway_on_random_port(
        "
    [gateway.auth]
    enabled = true
    ",
        None,
    )
    .await;

    let embedded_client = make_embedded_gateway_with_config_and_postgres(
        "
    [gateway.auth]
    enabled = true
    ",
    )
    .await;

    let postgres_pool = embedded_client
        .get_app_state_data()
        .unwrap()
        .postgres_connection_info
        .get_alpha_pool()
        .unwrap();

    let disabled_key =
        tensorzero_auth::postgres::create_key("my_org", "my_workspace", None, postgres_pool)
            .await
            .unwrap();

    let disabled_at = tensorzero_auth::postgres::disable_key(
        &TensorZeroApiKey::parse(disabled_key.expose_secret())
            .unwrap()
            .public_id,
        postgres_pool,
    )
    .await
    .unwrap();

    // TODO - come up with a way of listing all routes.
    // For now, we just check a handful of routes - our auth middleware is applied at the top level,
    // so it should be very difficult for us to accidentally skip a route
    let auth_required_routes = [
        ("POST", "/inference"),
        ("POST", "/batch_inference"),
        ("GET", "/batch_inference/fake-batch-id"),
        (
            "GET",
            "/batch_inference/fake-batch-id/inference/fake-inference-id",
        ),
        ("POST", "/feedback"),
        ("GET", "/metrics"),
        ("GET", "/internal/object_storage"),
        ("GET", "/v1/datasets/get_datapoints"),
    ];

    for (method, path) in auth_required_routes {
        // Authorization runs before we do any parsing of the request parameters/body,
        // so we don't need to provide a valid request here.
        let response = reqwest::Client::new()
            .request(
                Method::from_str(method).unwrap(),
                format!("http://{}/{}", child_data.addr, path),
            )
            .send()
            .await
            .unwrap();

        let status = response.status();
        let text = response.text().await.unwrap();
        assert_eq!(
            text,
            "{\"error\":\"TensorZero authentication error: Authorization header is required\"}"
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let bad_auth_response = reqwest::Client::new()
            .request(
                Method::from_str(method).unwrap(),
                format!("http://{}/{}", child_data.addr, path),
            )
            .header(http::header::AUTHORIZATION, "bad-header-value")
            .send()
            .await
            .unwrap();

        let status = bad_auth_response.status();
        let text = bad_auth_response.text().await.unwrap();
        assert_eq!(
            text,
            "{\"error\":\"TensorZero authentication error: Authorization header must start with 'Bearer '\"}"
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let bad_key_format_response = reqwest::Client::new()
            .request(
                Method::from_str(method).unwrap(),
                format!("http://{}/{}", child_data.addr, path),
            )
            .header(http::header::AUTHORIZATION, "Bearer invalid-key-format")
            .send()
            .await
            .unwrap();

        let status = bad_key_format_response.status();
        let text = bad_key_format_response.text().await.unwrap();
        assert_eq!(
            text,
            "{\"error\":\"TensorZero authentication error: Invalid API key: Invalid format for TensorZero API key: API key must be of the form `sk-t0-<public_id>-<long_key>`\"}"
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let missing_key_response = reqwest::Client::new()
            .request(
                Method::from_str(method).unwrap(),
                format!("http://{}/{}", child_data.addr, path),
            )
            .header(
                http::header::AUTHORIZATION,
                "Bearer sk-t0-aaaaaaaaaaaa-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            )
            .send()
            .await
            .unwrap();

        let status = missing_key_response.status();
        let text = missing_key_response.text().await.unwrap();
        assert_eq!(
            text,
            "{\"error\":\"TensorZero authentication error: Provided API key does not exist in the database\"}"
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let disabled_key_response = reqwest::Client::new()
            .request(
                Method::from_str(method).unwrap(),
                format!("http://{}/{}", child_data.addr, path),
            )
            .header(
                http::header::AUTHORIZATION,
                format!("Bearer {}", disabled_key.expose_secret()),
            )
            .send()
            .await
            .unwrap();

        let status = disabled_key_response.status();
        let text = disabled_key_response.text().await.unwrap();
        assert_eq!(
            text,
            format!("{{\"error\":\"TensorZero authentication error: API key was disabled at: {disabled_at}\"}}"),
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn test_create_api_key_cli() {
    // This test verifies that the --create-api-key CLI command works correctly
    let output = Command::new(GATEWAY_PATH)
        .args(["--create-api-key"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .unwrap();

    assert!(
        output.status.success(),
        "CLI command failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let api_key = stdout.trim();

    // Verify the key has the correct format
    assert!(
        api_key.starts_with("sk-t0-"),
        "API key should start with 'sk-t0-', got: {api_key}"
    );

    // Verify the key can be parsed
    let parsed_key = TensorZeroApiKey::parse(api_key);
    assert!(
        parsed_key.is_ok(),
        "API key should be valid, got error: {:?}",
        parsed_key.err()
    );

    // Verify the key works for authentication
    let child_data = start_gateway_on_random_port(
        "
    [gateway.auth]
    enabled = true
    ",
        None,
    )
    .await;

    let inference_response = reqwest::Client::new()
        .post(format!("http://{}/inference", child_data.addr))
        .header(http::header::AUTHORIZATION, format!("Bearer {api_key}"))
        .json(&json!({
            "model_name": "dummy::good",
            "input": {
                "messages": [
                    {
                        "role": "user",
                        "content": "Hello, world!",
                    }
                ]
            }
        }))
        .send()
        .await
        .unwrap();

    let status = inference_response.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "Created API key should work for authentication"
    );
}
