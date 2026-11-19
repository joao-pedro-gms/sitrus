//! Round-trip real do sidecar via Node.
//!
//! O teste `full_contract_in_mock_mode` sobe `engine/sidecar.js` pelo Node,
//! autentica por token e roda a mesma suíte de contrato do mock Rust —
//! exigindo `npm ci --prefix engine` antes. O teste do modo Electron exige
//! o binário castlabs instalado e sessão com display.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::run_contract;
use sitrus_engine::rpc::{SidecarConfig, SidecarMode, SidecarPage};
use sitrus_engine::{Engine as _, SidecarEngine};

/// Diretório do sidecar: override por env, senão `engine/` na raiz do repo.
/// O caminho canônico resolve o `../../` (o cwd dos testes é o crate).
fn engine_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SITRUS_ENGINE_DIR") {
        return PathBuf::from(dir);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine");
    path.canonicalize().unwrap_or(path)
}

/// Falha de spawn com Node inexistente é `SidecarUnavailable` (não ignora
/// o estado do `engine/`: roda em qualquer checkout).
#[test]
fn spawn_without_node_is_a_typed_sidecar_unavailable() {
    let config = SidecarConfig {
        node_path: "/nonexistent/sitrus-node".to_owned(),
        engine_dir: engine_dir(),
        rpc_timeout: Duration::from_secs(1),
        spawn_timeout: Duration::from_secs(1),
        ..SidecarConfig::default()
    };
    let result = SidecarEngine::spawn(&config);
    let error = result.expect_err("spawn must fail without node");
    assert!(error.requires_native_fallback());
}

/// Contrato completo contra `sidecar.js` em modo mock (Node puro, sem display).
#[test]
#[ignore = "requires node + `npm ci --prefix engine` (ws dependency)"]
fn full_contract_in_mock_mode() {
    let config = SidecarConfig {
        engine_dir: engine_dir(),
        mode: SidecarMode::Mock,
        page: SidecarPage::Mock,
        rpc_timeout: Duration::from_secs(5),
        spawn_timeout: Duration::from_secs(15),
        tick_ms: Some(10),
        ..SidecarConfig::default()
    };
    let engine =
        SidecarEngine::spawn(&config).expect("sidecar mock mode must spawn and authenticate");
    run_contract(&engine);
}

/// Modo Electron completo com página mock: valida o bridge stdin/stdout e o
/// ciclo de vida do BrowserWindow sem sessão Apple.
#[test]
#[ignore = "requires node, castlabs electron dist (`npm ci --prefix engine`) and a display session"]
fn electron_mode_round_trip_with_mock_page() {
    let config = SidecarConfig {
        engine_dir: engine_dir(),
        mode: SidecarMode::Electron,
        page: SidecarPage::Mock,
        rpc_timeout: Duration::from_secs(60),
        spawn_timeout: Duration::from_secs(60),
        handshake_timeout: Duration::from_secs(15),
        tick_ms: Some(25),
        electron_args: vec!["--no-sandbox".to_owned()],
        ..SidecarConfig::default()
    };
    let engine = SidecarEngine::spawn(&config)
        .expect("electron sidecar must spawn (CDM incl.) and authenticate");

    // Ciclo reduzido: uma faixa simulada de 600 ms vai de Playing a Ended.
    engine
        .load_queue(vec![sitrus_engine::QueueItem {
            id: "mock-electron-1".to_owned(),
            duration_ms: Some(600),
            ..Default::default()
        }])
        .expect("load in electron mode");
    engine.play().expect("play in electron mode");
    let events = engine.events();
    let ended = common::wait_for(events, Duration::from_secs(15), |event| {
        matches!(event, sitrus_engine::EngineEvent::Ended)
    });
    assert!(
        ended.is_some(),
        "mock page must emit Ended in electron mode"
    );

    // Volume válido passa; a sessão fecha sem pendurar o processo
    // (o Drop fecha stdin e o reaper conclui o kill sem bloquear).
    engine.set_volume(0.5).expect("volume in electron mode");
    let pid = engine.process_id().expect("sidecar pid alive");
    drop(engine);
    assert!(
        common::wait_process_gone(pid, Duration::from_secs(5)),
        "sidecar process must be reaped after drop (pid {pid})"
    );
}

/// Modo Electron com página apple-mock + SESSÃO Apple transportada: prova a
/// ponte de sessão ponta a ponta — stdin (Rust → sidecar), arquivo 0600
/// (sidecar → Electron), injeção por `session.defaultSession.cookies.set`
/// ANTES do loadURL, observada por `session.defaultSession.cookies.get` via
/// `session_probe`, e o adapter in-page acionando o player (facade MusicKit)
/// até `Ended`. (Blocker B1 — sem sessão Apple real nesta estação, a página
/// REAL e o licenciamento Widevine seguem não provados.)
#[test]
#[ignore = "requires node, castlabs electron dist (`npm ci --prefix engine`) and a display session"]
fn electron_mode_round_trip_with_session_and_apple_mock_page() {
    const COOKIE_VALUE: &str = "SITRUS-E2E-COOKIE-4d21a19f";
    const COOKIE_NAME: &str = "citm";
    const MEDIA_TOKEN: &str = "SITRUS-E2E-MEDIA-TOKEN-b8327e0a";

    let config = SidecarConfig {
        engine_dir: engine_dir(),
        mode: SidecarMode::Electron,
        page: SidecarPage::AppleMock,
        rpc_timeout: Duration::from_secs(60),
        spawn_timeout: Duration::from_secs(60),
        handshake_timeout: Duration::from_secs(15),
        tick_ms: Some(25),
        electron_args: vec!["--no-sandbox".to_owned()],
        session: Some(sitrus_engine::rpc::SidecarSession {
            media_user_token: Some(MEDIA_TOKEN.to_owned()),
            cookies: vec![
                sitrus_engine::rpc::SidecarCookie {
                    name: COOKIE_NAME.to_owned(),
                    value: COOKIE_VALUE.to_owned(),
                    domain: Some(".apple.com".to_owned()),
                },
                sitrus_engine::rpc::SidecarCookie {
                    name: "geo".to_owned(),
                    value: "br".to_owned(),
                    domain: None,
                },
            ],
        }),
        ..SidecarConfig::default()
    };
    let engine =
        SidecarEngine::spawn(&config).expect("electron sidecar com sessão deve subir e autenticar");

    // O cookies.get OBSERVADO no perfil do Electron (prova da injeção).
    let probe = engine.session_probe().expect("session_probe on electron");
    let injected = probe
        .cookies
        .iter()
        .find(|cookie| cookie.name == COOKIE_NAME)
        .expect("injected cookie must be observed by cookies.get");
    assert_eq!(injected.value, COOKIE_VALUE, "cookie value round-trip");
    assert_eq!(injected.domain.as_deref(), Some(".apple.com"));
    assert_eq!(probe.media_user_token.as_deref(), Some(MEDIA_TOKEN));

    // Superfície de processo do Node: o segredo não está em argv/env.
    let pid = engine.process_id().expect("sidecar pid alive");
    for surface in ["cmdline", "environ"] {
        let bytes = std::fs::read(format!("/proc/{pid}/{surface}")).unwrap_or_default();
        assert!(
            !bytes
                .windows(COOKIE_VALUE.len())
                .any(|window| window == COOKIE_VALUE.as_bytes()),
            "cookie value leaked into /proc/{pid}/{surface}"
        );
        assert!(
            !bytes
                .windows(MEDIA_TOKEN.len())
                .any(|window| window == MEDIA_TOKEN.as_bytes()),
            "media token leaked into /proc/{pid}/{surface}"
        );
    }
    let stderr = engine.stderr_tail().join("\n");
    assert!(
        !stderr.contains(COOKIE_VALUE) && !stderr.contains(MEDIA_TOKEN),
        "sidecar stderr must stay secret-free: {stderr}"
    );

    // Adapter in-page aciona o player (facade MusicKit da página apple-mock).
    engine
        .load_queue(vec![sitrus_engine::QueueItem {
            id: "am-facade-1".to_owned(),
            duration_ms: Some(600),
            ..Default::default()
        }])
        .expect("load via page adapter");
    engine.play().expect("play via page adapter");
    let events = engine.events();
    let ended = common::wait_for(events, Duration::from_secs(20), |event| {
        matches!(event, sitrus_engine::EngineEvent::Ended)
    });
    assert!(ended.is_some(), "adapter must drive the player to Ended");

    // Sessão fecha sem pendurar o processo (reaper detached).
    let pid = engine.process_id().expect("sidecar pid alive");
    drop(engine);
    assert!(
        common::wait_process_gone(pid, Duration::from_secs(5)),
        "sidecar process must be reaped after drop (pid {pid})"
    );
}
