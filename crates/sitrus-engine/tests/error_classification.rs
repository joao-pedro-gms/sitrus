//! Classificação de erros do engine: tabela de categorias e decisão de fallback.

use std::time::Duration;

use sitrus_engine::{
    Engine, EngineError, EngineErrorKind, MockEngine, QueueItem, SidecarConfig, SidecarEngine,
};

fn representative_errors() -> Vec<(EngineError, EngineErrorKind, bool)> {
    vec![
        (
            EngineError::VolumeOutOfRange(1.5),
            EngineErrorKind::InvalidInput,
            false,
        ),
        (
            EngineError::InvalidQueueItem {
                reason: "no id".to_owned(),
            },
            EngineErrorKind::InvalidInput,
            false,
        ),
        (
            EngineError::SessionInvalid {
                reason: "neither cookies nor a media user token".to_owned(),
            },
            EngineErrorKind::InvalidInput,
            false,
        ),
        (
            EngineError::SidecarUnavailable {
                reason: "node missing".to_owned(),
            },
            EngineErrorKind::SidecarUnavailable,
            true,
        ),
        (
            EngineError::SidecarTimedOut { timeout_ms: 10_000 },
            EngineErrorKind::SidecarUnavailable,
            true,
        ),
        (
            EngineError::ConnectionLost,
            EngineErrorKind::SidecarUnavailable,
            true,
        ),
        (
            EngineError::HandshakeRejected {
                reason: "bad token".to_owned(),
            },
            EngineErrorKind::Handshake,
            true,
        ),
        (
            EngineError::Rpc {
                code: -32601,
                message: "unknown method".to_owned(),
            },
            EngineErrorKind::Rpc,
            false,
        ),
        (
            EngineError::RpcTimedOut { timeout_ms: 5_000 },
            EngineErrorKind::Rpc,
            false,
        ),
        (
            EngineError::InvalidMessage {
                reason: "garbage".to_owned(),
            },
            EngineErrorKind::Protocol,
            true,
        ),
    ]
}

#[test]
fn error_kinds_and_fallback_signals_follow_the_policy_table() {
    for (error, expected_kind, expected_fallback) in representative_errors() {
        assert_eq!(error.kind(), expected_kind, "kind of {error:?}");
        assert_eq!(
            error.requires_native_fallback(),
            expected_fallback,
            "fallback of {error:?}"
        );
    }
}

#[test]
fn rpc_error_responses_map_to_the_typed_variants() {
    let token_rejected = EngineError::from_rpc_error(-32001, "invalid session token");
    assert!(matches!(
        token_rejected,
        EngineError::HandshakeRejected { .. }
    ));
    assert!(token_rejected.requires_native_fallback());

    let method_missing = EngineError::from_rpc_error(-32601, "unknown method");
    assert_eq!(
        method_missing,
        EngineError::Rpc {
            code: -32601,
            message: "unknown method".to_owned(),
        }
    );
    assert!(!method_missing.requires_native_fallback());
}

#[test]
fn spawn_with_missing_node_binary_reports_sidecar_unavailable() {
    // engine_dir aponta para o sidecar do repo; o binário Node não existe.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine_dir = manifest_dir
        .join("../../engine")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../engine"));
    let config = SidecarConfig {
        node_path: "/nonexistent/sitrus-node".to_owned(),
        engine_dir,
        rpc_timeout: Duration::from_secs(1),
        spawn_timeout: Duration::from_secs(1),
        ..SidecarConfig::default()
    };
    let result = SidecarEngine::spawn(&config);
    let error = result.expect_err("spawn must fail without a node binary and without engine/");
    assert!(
        matches!(
            error.kind(),
            EngineErrorKind::SidecarUnavailable | EngineErrorKind::Protocol
        ),
        "spawn failure without engine/ or node is unavailability/protocol: {error:?}"
    );
    assert!(
        error.requires_native_fallback(),
        "app must fall back to sitrus-player: {error:?}"
    );
}

#[test]
fn invalid_input_never_triggers_the_native_fallback() {
    let engine = MockEngine::new();
    let error = engine
        .set_volume(2.0)
        .expect_err("volume 2.0 must be rejected");
    assert_eq!(error.kind(), EngineErrorKind::InvalidInput);
    assert!(!error.requires_native_fallback());

    let error = engine
        .load_queue(vec![QueueItem::new("")])
        .expect_err("empty id must be rejected");
    assert_eq!(error.kind(), EngineErrorKind::InvalidInput);
    assert!(!error.requires_native_fallback());
}
