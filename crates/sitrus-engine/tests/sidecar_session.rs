//! Transporte da sessão Apple pelo sidecar (blocker B1 — parte Rust).
//!
//! Prova, contra `engine/sidecar.js` real (modo mock, sem display):
//!
//! 1. o envelope JSON sai pelo **stdin** do processo (`SidecarEngine` →
//!    sidecar) e o sidecar o recebe íntegro (`session_probe` devolve o echo);
//! 2. o valor do cookie e o media-user-token **não aparecem** em
//!    `/proc/<pid>/cmdline` nem `/proc/<pid>/environ` do processo Node;
//! 3. nada do segredo é logado (cauda de stderr do sidecar limpa).
//!
//! Ignorado por padrão: exige Node + `npm ci --prefix engine` (dep `ws`).

#![cfg(unix)]

mod common;

use std::time::Duration;

use common::wait_for;
use sitrus_engine::rpc::{SidecarConfig, SidecarCookie, SidecarMode, SidecarPage, SidecarSession};
use sitrus_engine::{Engine as _, EngineEvent, SidecarEngine};

/// Diretório do sidecar (idem `sidecar_e2e.rs`).
fn engine_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("SITRUS_ENGINE_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine");
    path.canonicalize().unwrap_or(path)
}

/// `true` quando SOME byte de `needle` no arquivo de superfície do processo
/// (`cmdline` ou `environ`, ambos NUL-separados).
fn process_surface_contains(pid: Option<u32>, needle: &[u8]) -> bool {
    let Some(pid) = pid else {
        // Sem PID não há superfície a examinar: o teste trata como vazamento.
        return needle.is_empty();
    };
    for surface in ["cmdline", "environ"] {
        let bytes = std::fs::read(format!("/proc/{pid}/{surface}")).unwrap_or_default();
        if !needle.is_empty() && bytes.windows(needle.len()).any(|window| window == needle) {
            return true;
        }
    }
    false
}

/// Round-trip da sessão: stdin → sidecar (echo no probe) e ausência do
/// segredo em argv/env/log do processo sidecar.
#[test]
#[ignore = "requires node + `npm ci --prefix engine` (ws dependency)"]
fn session_reaches_the_sidecar_without_leaking_into_argv_env_or_log() {
    const COOKIE_VALUE: &str = "SITRUS-E2E-COOKIE-4d21a19f";
    const COOKIE_NAME: &str = "citm";
    const MEDIA_TOKEN: &str = "SITRUS-E2E-MEDIA-TOKEN-b8327e0a";

    let config = SidecarConfig {
        engine_dir: engine_dir(),
        mode: SidecarMode::Mock,
        page: SidecarPage::Mock,
        rpc_timeout: Duration::from_secs(5),
        spawn_timeout: Duration::from_secs(15),
        tick_ms: Some(10),
        session: Some(SidecarSession {
            media_user_token: Some(MEDIA_TOKEN.to_owned()),
            cookies: vec![
                SidecarCookie {
                    name: COOKIE_NAME.to_owned(),
                    value: COOKIE_VALUE.to_owned(),
                    domain: Some(".apple.com".to_owned()),
                },
                SidecarCookie {
                    name: "geo".to_owned(),
                    value: "br".to_owned(),
                    domain: None,
                },
            ],
        }),
        ..SidecarConfig::default()
    };
    let engine = SidecarEngine::spawn(&config)
        .expect("sidecar (modo mock, com sessão) deve subir e autenticar");

    // O round-trip completo da sessão: o echo inclui os cookies e o token.
    let probe = engine
        .session_probe()
        .expect("session_probe must echo the received envelope");
    let citm = probe
        .cookies
        .iter()
        .find(|cookie| cookie.name == COOKIE_NAME)
        .expect("cookie citm must round-trip");
    assert_eq!(citm.value, COOKIE_VALUE, "cookie value round-trips intact");
    assert_eq!(citm.domain.as_deref(), Some(".apple.com"));
    assert!(
        probe
            .cookies
            .iter()
            .any(|cookie| cookie.name == "geo" && cookie.value == "br"),
        "cookie without explicit domain round-trips as sent"
    );
    assert_eq!(probe.media_user_token.as_deref(), Some(MEDIA_TOKEN));

    // Superfície de processo: o segredo não pode estar em cmdline/environ do
    // Node. (O token do WS NÃO é varrido aqui: ele vai por env por design.)
    let pid = engine.process_id().expect("sidecar must have a pid");
    assert!(
        !process_surface_contains(engine.process_id(), COOKIE_VALUE.as_bytes()),
        "cookie value must not appear in /proc/{pid}/cmdline or environ"
    );
    assert!(
        !process_surface_contains(Some(pid), MEDIA_TOKEN.as_bytes()),
        "media user token must not appear in /proc/{pid}/cmdline or environ"
    );

    // Log: a cauda de stderr do sidecar nunca carrega o segredo.
    let tail = engine.stderr_tail().join("\n");
    assert!(
        !tail.contains(COOKIE_VALUE) && !tail.contains(MEDIA_TOKEN),
        "sidecar stderr must stay secret-free: {tail}"
    );

    // A sessão não interfere na trait: pipeline normal funciona com ela.
    engine
        .load_queue(vec![sitrus_engine::QueueItem {
            id: "session-1".to_owned(),
            duration_ms: Some(300),
            ..Default::default()
        }])
        .expect("load with session present");
    engine.play().expect("play with session present");
    let ended = wait_for(engine.events(), Duration::from_secs(8), |event| {
        matches!(event, EngineEvent::Ended)
    });
    assert!(ended.is_some(), "playback must run with a session attached");
    drop(engine);
}
