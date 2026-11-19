//! Contrato da trait [`Engine`] executado contra a implementação em Rust.

mod common;

use common::run_contract;
use sitrus_engine::Engine;
use sitrus_engine::MockEngine;

/// O `MockEngine` é o oráculo determinístico do contrato sem Node/Electron.
#[test]
fn mock_engine_satisfies_the_engine_contract() {
    let engine = MockEngine::new();
    run_contract(&engine);
}

/// Fila vazia limpa o carregado: nenhum Duration novo após carregar `[]`.
#[test]
fn mock_engine_load_queue_with_empty_list_clears_the_queue() {
    let engine = MockEngine::new();
    engine
        .load_queue(vec![sitrus_engine::QueueItem::new("only")])
        .expect("initial load");
    engine
        .load_queue(Vec::new())
        .expect("empty load_queue is valid");
    assert_eq!(engine.state(), sitrus_engine::EngineState::Stopped);
}

/// Item sem identificador é rejeitado antes de tocar a fila.
#[test]
fn mock_engine_rejects_queue_items_without_id() {
    let engine = MockEngine::new();
    let invalid = sitrus_engine::QueueItem::new("   ");
    assert!(matches!(
        engine.load_queue(vec![invalid]),
        Err(sitrus_engine::EngineError::InvalidQueueItem { .. })
    ));
}
