//! Auxiliares compartilhados pelos testes de integração: espera de eventos e
//! a suíte de contrato genérica, executada contra o `MockEngine` Rust e
//! contra o sidecar real (modo mock de Node e modo Electron com página mock).

#![allow(dead_code)]

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use sitrus_engine::{Engine, EngineEvent, EngineState, QueueItem};

/// Intervalo de polling do canal de eventos nos testes.
const POLL: Duration = Duration::from_millis(25);

/// Espera o primeiro evento que casa com `pred` dentro do prazo.
pub fn wait_for(
    events: &Receiver<EngineEvent>,
    timeout: Duration,
    pred: impl Fn(&EngineEvent) -> bool,
) -> Option<EngineEvent> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        match events.recv_timeout(remaining.min(POLL)) {
            Ok(event) if pred(&event) => return Some(event),
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => panic!("event channel closed"),
        }
    }
}

/// Coleta eventos por uma janela de tempo sem esperar nada específico.
pub fn collect_for(events: &Receiver<EngineEvent>, window: Duration) -> Vec<EngineEvent> {
    let mut collected = Vec::new();
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        match events.recv_timeout(window.min(POLL)) {
            Ok(event) => collected.push(event),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    collected
}

/// Espera o processo sumir do `/proc` (o Drop do `SidecarEngine` fecha o
/// stdin e o reaper detached conclui o kill em até ~600 ms — sem bloquear o
/// thread que derrubou o engine).
pub fn wait_process_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !std::path::Path::new(&format!("/proc/{pid}/cmdline")).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !std::path::Path::new(&format!("/proc/{pid}/cmdline")).exists()
}

/// Suíte de contrato da trait, exigida de qualquer implementação.
pub fn run_contract(engine: &dyn Engine) {
    // Estado inicial.
    assert_eq!(engine.state(), EngineState::Stopped, "initial state");

    // Play sem fila: Ok com evento de erro, estado intocado.
    engine.play().expect("play on empty queue is Ok");
    let error_at = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Error(_))
    });
    assert!(error_at.is_some(), "expected an error event without queue");
    assert_eq!(engine.state(), EngineState::Stopped);

    // Carga da fila com duas faixas de 300 ms.
    engine
        .load_queue(vec![
            QueueItem {
                id: "item-1".to_owned(),
                title: Some("First".to_owned()),
                duration_ms: Some(300),
                url: None,
            },
            QueueItem {
                id: "item-2".to_owned(),
                duration_ms: Some(300),
                ..QueueItem::default()
            },
        ])
        .expect("load_queue with valid items");
    let position_reset = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Position(_))
    })
    .expect("position reset after load");
    assert_eq!(position_reset, EngineEvent::Position(Duration::ZERO));
    let duration_seen = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Duration(_))
    });
    assert_eq!(
        duration_seen,
        Some(EngineEvent::Duration(Duration::from_millis(300))),
        "duration of the first item"
    );
    assert_eq!(engine.state(), EngineState::Stopped);

    // Play: Playing e posições crescentes.
    engine.play().expect("play command");
    let playing = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::StateChanged(EngineState::Playing))
    });
    assert!(playing.is_some(), "expected the Playing transition");
    let first_position = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Position(_))
    })
    .expect("first position while playing");
    let second_position = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Position(_))
    })
    .expect("second position while playing");
    let EngineEvent::Position(first) = first_position else {
        unreachable!("covered by the predicate above")
    };
    let EngineEvent::Position(second) = second_position else {
        unreachable!("covered by the predicate above")
    };
    assert!(
        second > first,
        "position must advance while playing: {first:?} -> {second:?}"
    );

    // Volume: fora do intervalo é erro síncrono; dentro é Ok.
    assert!(
        matches!(
            engine.set_volume(1.5),
            Err(sitrus_engine::EngineError::VolumeOutOfRange(_))
        ),
        "volume above 1.0 must be rejected"
    );
    assert!(
        matches!(
            engine.set_volume(-0.1),
            Err(sitrus_engine::EngineError::VolumeOutOfRange(_))
        ),
        "negative volume must be rejected"
    );
    assert!(
        matches!(
            engine.set_volume(f32::NAN),
            Err(sitrus_engine::EngineError::VolumeOutOfRange(_))
        ),
        "NaN volume must be rejected"
    );
    engine.set_volume(0.42).expect("valid volume accepted");

    // Pause: Paused e posições congeladas.
    engine.pause().expect("pause command");
    let paused = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::StateChanged(EngineState::Paused))
    });
    assert!(paused.is_some(), "expected the Paused transition");
    let frozen = collect_for(engine.events(), Duration::from_millis(400));
    let stuck: Vec<_> = frozen
        .iter()
        .filter(|event| matches!(event, EngineEvent::Position(_)))
        .cloned()
        .collect();
    assert!(
        stuck.all_duplicates(),
        "position must freeze while paused: {stuck:?}"
    );
    assert_eq!(engine.state(), EngineState::Paused);

    // Seek com clamp: 100 ms é aceito; posição chega.
    engine
        .seek(Duration::from_millis(100))
        .expect("seek command");
    let seeked = wait_for(
        engine.events(),
        Duration::from_secs(3),
        |event| matches!(event, EngineEvent::Position(x) if *x == Duration::from_millis(100)),
    );
    assert_eq!(
        seeked,
        Some(EngineEvent::Position(Duration::from_millis(100))),
        "seek reports the requested position"
    );

    // Retoma e chega ao fim: Ended, avanço para o item-2 (nova Duration).
    engine.play().expect("resume");
    wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::StateChanged(EngineState::Playing))
    })
    .expect("Playing again");
    let ended = wait_for(engine.events(), Duration::from_secs(8), |event| {
        matches!(event, EngineEvent::Ended)
    });
    assert!(ended.is_some(), "expected Ended for item-1");
    let advanced = wait_for(engine.events(), Duration::from_secs(3), |event| {
        matches!(event, EngineEvent::Duration(_))
    });
    assert_eq!(
        advanced,
        Some(EngineEvent::Duration(Duration::from_millis(300))),
        "engine advances to item-2 after Ended"
    );
    assert_eq!(engine.state(), EngineState::Stopped);

    // Segunda faixa também termina; sem item-3, nenhuma Duration nova.
    engine.play().expect("play item-2");
    let ended_again = wait_for(engine.events(), Duration::from_secs(8), |event| {
        matches!(event, EngineEvent::Ended)
    });
    assert!(ended_again.is_some(), "expected Ended for item-2");
    let quiet = collect_for(engine.events(), Duration::from_millis(500));
    assert!(
        !quiet
            .iter()
            .any(|event| matches!(event, EngineEvent::Duration(_))),
        "no further duration when the queue ends: {quiet:?}"
    );
    assert_eq!(engine.state(), EngineState::Stopped);
}

trait AllDuplicates {
    fn all_duplicates(&self) -> bool;
}

impl<T: PartialEq> AllDuplicates for [T] {
    fn all_duplicates(&self) -> bool {
        self.windows(2).all(|window| window[0] == window[1])
    }
}
