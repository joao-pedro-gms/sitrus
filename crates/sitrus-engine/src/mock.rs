//! Implementação de referência da trait em Rust, para testes de contrato.
//!
//! Semântica espelhada no worker do `sitrus-player`: comandos por canal, um
//! tick dedicado, eventos publicados no `Receiver`. Não toca áudio nem
//! depende de Node/Electron — serve como oráculo determinístico do contrato
//! [`Engine`] tanto nos testes Rust quanto (em paridade) no mock de Node do
//! sidecar validado pelos testes e2e.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SendError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::engine::{Engine, EngineEvent, EngineState, QueueItem, validate_volume};
use crate::error::EngineError;

/// Intervalo padrão entre eventos de posição.
const DEFAULT_POSITION_INTERVAL: Duration = Duration::from_millis(15);
/// Duração simulada quando o item não declara `duration_ms`.
const DEFAULT_SIMULATED_DURATION_MS: u64 = 3_000;

#[derive(Debug)]
enum Command {
    LoadQueue(Vec<QueueItem>),
    Play,
    Pause,
    Seek(Duration),
    SetVolume(f32),
    Shutdown,
}

/// Simuladora determinística do contrato de engine.
#[derive(Debug)]
pub struct MockEngine {
    command_sender: Sender<Command>,
    event_receiver: Receiver<EngineEvent>,
    state: Arc<Mutex<EngineState>>,
}

impl MockEngine {
    /// Cria a simuladora com tick padrão de 15 ms.
    #[must_use]
    pub fn new() -> Self {
        Self::with_position_interval(DEFAULT_POSITION_INTERVAL)
    }

    /// Cria a simuladora com o intervalo de posição indicado.
    #[must_use]
    pub fn with_position_interval(interval: Duration) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let worker_events = event_sender.clone();
        let state = Arc::new(Mutex::new(EngineState::Stopped));
        let worker_state = Arc::clone(&state);
        let spawn_result = thread::Builder::new()
            .name("sitrus-mock-engine".to_owned())
            .spawn(move || {
                let mut worker = MockWorker::new(worker_events, interval, worker_state);
                run_worker(command_receiver, &mut worker);
            });
        // thread::spawn com nome customizado só falha sem recursos do SO;
        // aí não há contrato a provar — propagar via evento.
        if spawn_result.is_err() {
            let _ = event_sender.send(EngineEvent::Error(
                "could not start mock engine worker".to_owned(),
            ));
        }
        Self {
            command_sender,
            event_receiver,
            state,
        }
    }

    fn send(&self, command: Command) -> Result<(), EngineError> {
        self.command_sender
            .send(command)
            .map_err(|SendError(_)| EngineError::ConnectionLost)
    }
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for MockEngine {
    fn load_queue(&self, queue: Vec<QueueItem>) -> Result<(), EngineError> {
        for item in &queue {
            item.validate()?;
        }
        self.send(Command::LoadQueue(queue))
    }

    fn play(&self) -> Result<(), EngineError> {
        self.send(Command::Play)
    }

    fn pause(&self) -> Result<(), EngineError> {
        self.send(Command::Pause)
    }

    fn seek(&self, position: Duration) -> Result<(), EngineError> {
        self.send(Command::Seek(position))
    }

    fn set_volume(&self, volume: f32) -> Result<(), EngineError> {
        validate_volume(volume)?;
        self.send(Command::SetVolume(volume))
    }

    fn state(&self) -> EngineState {
        self.state
            .lock()
            .map(|guard| *guard)
            .unwrap_or(EngineState::Stopped)
    }

    fn events(&self) -> &Receiver<EngineEvent> {
        &self.event_receiver
    }
}

impl Drop for MockEngine {
    fn drop(&mut self) {
        let _ = self.command_sender.send(Command::Shutdown);
    }
}

struct MockWorker {
    events: Sender<EngineEvent>,
    state: EngineState,
    state_slot: Arc<Mutex<EngineState>>,
    queue: Vec<QueueItem>,
    index: usize,
    position: Duration,
    duration: Duration,
    volume: f32,
    interval: Duration,
    last_position_event: std::time::Instant,
}

impl MockWorker {
    fn new(
        events: Sender<EngineEvent>,
        interval: Duration,
        state_slot: Arc<Mutex<EngineState>>,
    ) -> Self {
        Self {
            events,
            state: EngineState::Stopped,
            state_slot,
            queue: Vec::new(),
            index: 0,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 1.0,
            interval,
            last_position_event: std::time::Instant::now(),
        }
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    fn transition(&mut self, next: EngineState) {
        if self.state != next {
            self.state = next;
            if let Ok(mut guard) = self.state_slot.lock() {
                *guard = next;
            }
            self.emit(EngineEvent::StateChanged(next));
        }
    }

    fn simulated_duration(&self, item: &QueueItem) -> Duration {
        Duration::from_millis(item.duration_ms.unwrap_or(DEFAULT_SIMULATED_DURATION_MS))
    }

    fn load_queue(&mut self, queue: Vec<QueueItem>) {
        self.index = 0;
        self.position = Duration::ZERO;
        self.transition(EngineState::Stopped);
        self.emit(EngineEvent::Position(Duration::ZERO));
        if let Some(first) = queue.first() {
            self.duration = self.simulated_duration(first);
            self.emit(EngineEvent::Duration(self.duration));
        } else {
            self.duration = Duration::ZERO;
        }
        self.queue = queue;
    }

    fn play(&mut self) {
        if self.state == EngineState::Playing {
            return;
        }
        if self.queue.is_empty() {
            self.emit(EngineEvent::Error("no queue item is loaded".to_owned()));
            return;
        }
        self.transition(EngineState::Playing);
        self.last_position_event = std::time::Instant::now();
        self.emit(EngineEvent::Position(self.position));
    }

    fn pause(&mut self) {
        if self.state != EngineState::Playing {
            return;
        }
        self.transition(EngineState::Paused);
        self.emit(EngineEvent::Position(self.position));
    }

    fn seek(&mut self, requested: Duration) {
        if self.queue.is_empty() {
            self.emit(EngineEvent::Error("no queue item is loaded".to_owned()));
            return;
        }
        let position = requested.min(self.duration);
        self.position = position;
        self.emit(EngineEvent::Position(position));
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }

    fn tick(&mut self) {
        if self.state != EngineState::Playing {
            return;
        }
        let elapsed = self.last_position_event.elapsed();
        if elapsed < self.interval {
            return;
        }
        self.last_position_event = std::time::Instant::now();
        self.position += elapsed.min(self.interval);
        if self.position >= self.duration {
            self.position = self.duration;
            self.emit(EngineEvent::Position(self.position));
            self.transition(EngineState::Stopped);
            self.emit(EngineEvent::Ended);
            self.advance();
            return;
        }
        self.emit(EngineEvent::Position(self.position));
    }

    /// Ao terminar, avança para o próximo item da fila (posição zero) e aguarda `play`.
    fn advance(&mut self) {
        let next = self.index + 1;
        if next < self.queue.len() {
            self.index = next;
            self.position = Duration::ZERO;
            self.duration = self.simulated_duration(&self.queue[next]);
            self.emit(EngineEvent::Position(Duration::ZERO));
            self.emit(EngineEvent::Duration(self.duration));
        }
    }

    fn process(&mut self, command: Command) -> bool {
        match command {
            Command::LoadQueue(queue) => self.load_queue(queue),
            Command::Play => self.play(),
            Command::Pause => self.pause(),
            Command::Seek(position) => self.seek(position),
            Command::SetVolume(volume) => self.set_volume(volume),
            Command::Shutdown => return false,
        }
        true
    }
}

fn run_worker(commands: Receiver<Command>, worker: &mut MockWorker) {
    loop {
        match commands.recv_timeout(worker.interval) {
            Ok(command) => {
                if !worker.process(command) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => worker.tick(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
