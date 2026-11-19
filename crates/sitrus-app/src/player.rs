//! Controlador do player: encaminha a reprodução para a Engine DRM quando
//! disponível e mantém `sitrus-player` para arquivos locais e prévias.
//!
//! O engine DRM é **preguiçoso** ([M2]): nenhum processo sobe no arranque —
//! o sidecar nasce na primeira faixa que exige DRM, em thread de fundo, com
//! o app já interativo; a falha de spawn vira erro visível (toast). Os
//! comandos no engine são enfileirados (nada de RPC síncrono no frame); as
//! falhas tipadas chegam como [`EngineEvent::CommandFailed`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use sitrus_core::player_state::{PlaybackStatus, PlayerState as CorePlayerState};
use sitrus_core::queue::PlaybackQueue;
use sitrus_engine::{
    Engine, EngineError, EngineEvent, EngineState, QueueItem, SidecarConfig, SidecarEngine,
    SidecarSession,
};
use sitrus_player::{Player, PlayerEvent};

use crate::model::{SongRow, secs_from_duration};

/// Seleção de backend da faixa corrente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBackend {
    /// Engine DRM/stream.
    Engine,
    /// Player nativo para arquivos locais e prévias.
    Native,
}

/// Resultado do spawn preguiçoso do sidecar (chega pelo canal de spawn).
enum SpawnOutcome {
    /// Engine pronto para uso, com a sessão transportada na config.
    Started(Box<dyn Engine + Send>, Option<SidecarSession>),
    /// Spawn falhou (erro tipado, vira aviso visível).
    Failed(EngineError),
}

/// Estado do spawn preguiçoso do engine DRM.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EngineSpawnStatus {
    /// Nenhum pedido ainda; sobe no primeiro comando que exige DRM.
    Dormant,
    /// Spawn em andamento na thread de fundo.
    Starting,
    /// Falhou (mensagem para a UI); nova tentativa no próximo uso de faixa DRM.
    Failed(String),
}

/// Assinatura do spawner injetável (testes trocam o processo real).
type EngineSpawner =
    Arc<dyn Fn(&SidecarConfig) -> Result<Box<dyn Engine + Send>, EngineError> + Send + Sync>;

/// Controlador de transporte + fila.
pub struct EngineController {
    native: Player,
    engine: Option<Box<dyn Engine + Send>>,
    active_backend: ActiveBackend,
    /// Spawn do sidecar é preguiçoso: nenhuma chamada a `spawn` de
    /// construção; `Dormant` até a primeira faixa sem URL/prévia.
    spawn_status: EngineSpawnStatus,
    /// Sessão Apple a transportar no próximo spawn de sidecar.
    pending_session: Option<SidecarSession>,
    /// Sessão com a qual o engine ativo foi spawnado (re-spawn quando mudar).
    spawned_session: Option<SidecarSession>,
    /// Spawn concluído (thread de fundo) → drenado por frame.
    spawn_rx: Receiver<SpawnOutcome>,
    spawn_tx: Sender<SpawnOutcome>,
    spawner: EngineSpawner,
    /// Faixas da fila (espelho para o painel de fila; a fonte de avanço
    /// é a `PlaybackQueue` de sitrus-core).
    queue_items: Vec<SongRow>,
    queue: PlaybackQueue,
    /// Estado de domínio (status/volume/duração) espelhado dos eventos.
    state: CorePlayerState,
    position_secs: u32,
    title: String,
    subtitle: String,
    current_key: Option<String>,
    /// Índice da faixa atual dentro de `queue_items`, quando encontrável.
    current_index: Option<usize>,
    /// Último id enviado ao backend (evita recarregar a mesma URL).
    loaded_id: Option<String>,
    /// Erro da última tentativa, para a UI exibir e não reapresentar em loop.
    last_error: Option<String>,
    /// Erros de caminhos síncronos aguardando o próximo drain (toast).
    pending_errors: Vec<String>,
}

impl EngineController {
    /// Cria o controlador SEM tocar em processo nenhum (M2): o engine DRM
    /// sobe preguiçosamente, em thread de fundo, na primeira faixa que
    /// exige DRM; o player nativo fica pronto para prévias e arquivos
    /// locais desde o arranque.
    #[must_use]
    pub fn new() -> Self {
        Self::with_engine(None)
    }

    fn with_engine(engine: Option<Box<dyn Engine + Send>>) -> Self {
        Self::with_parts(engine, None)
    }

    /// Construtor interno com spawner injetável (testes usam o próprio).
    fn with_parts(engine: Option<Box<dyn Engine + Send>>, spawner: Option<EngineSpawner>) -> Self {
        let (spawn_tx, spawn_rx) = std::sync::mpsc::channel();
        Self {
            native: Player::new(),
            engine,
            active_backend: ActiveBackend::Native,
            spawn_status: EngineSpawnStatus::Dormant,
            pending_session: None,
            spawned_session: None,
            spawn_rx,
            spawn_tx,
            spawner: spawner.unwrap_or_else(sidecar_spawner),
            queue_items: Vec::new(),
            queue: PlaybackQueue::new(),
            state: CorePlayerState::new(),
            position_secs: 0,
            title: String::new(),
            subtitle: String::new(),
            current_key: None,
            current_index: None,
            loaded_id: None,
            last_error: None,
            pending_errors: Vec::new(),
        }
    }

    /// Construtor de teste com spawner de sidecar substituível.
    #[cfg(test)]
    #[must_use]
    pub fn with_spawner(spawner: EngineSpawner) -> Self {
        Self::with_parts(None, Some(spawner))
    }

    /// Instala/atualiza a sessão Apple que o sidecar carrega. Se um engine
    /// vivo foi spawnado com outra sessão (ou sem sessão), ele é descartado
    /// (Drop não bloqueante) e a próxima faixa DRM responde com o spawn da
    /// sessão nova. `None` (logout) limpa.
    pub fn set_user_session(&mut self, session: Option<SidecarSession>) {
        if self.pending_session == session {
            return;
        }
        self.pending_session = session;
        if self.engine.is_some() && self.spawned_session != self.pending_session {
            if self.active_backend == ActiveBackend::Engine {
                self.active_backend = ActiveBackend::Native;
            }
            self.engine = None;
            self.spawned_session = None;
            self.spawn_status = EngineSpawnStatus::Dormant;
        }
    }

    /// Mensagem da última falha de spawn (para o aviso da UI), quando o
    /// engine ainda não está disponível e o usuário precisa saber do
    /// motivo. `None` quando não há falha pendente.
    #[must_use]
    pub fn engine_spawn_message(&self) -> Option<String> {
        if self.engine.is_some() {
            return None;
        }
        match &self.spawn_status {
            EngineSpawnStatus::Failed(reason) => Some(reason.clone()),
            _ => None,
        }
    }

    /// Pede o spawn do sidecar em thread de fundo (M2). O construtor
    /// continua interativo: o resultado chega pelo canal de spawn e é
    /// drenado por frame.
    fn begin_spawn(&mut self) {
        if matches!(self.spawn_status, EngineSpawnStatus::Starting) {
            return;
        }
        self.spawn_status = EngineSpawnStatus::Starting;
        let session = self.pending_session.clone();
        let spawner = Arc::clone(&self.spawner);
        let spawn_tx = self.spawn_tx.clone();
        let _ = std::thread::Builder::new()
            .name("sitrus-engine-spawn".to_owned())
            .spawn(move || {
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                        || match engine_directory() {
                            Some(engine_dir) => {
                                let config = SidecarConfig {
                                    engine_dir,
                                    session,
                                    show_window: engine_window_visible(),
                                    ..SidecarConfig::default()
                                };
                                match spawner(&config) {
                                    Ok(engine) => SpawnOutcome::Started(engine, config.session),
                                    Err(error) => SpawnOutcome::Failed(error),
                                }
                            }
                            None => SpawnOutcome::Failed(EngineError::SidecarUnavailable {
                                reason: "engine/ directory was not found".to_owned(),
                            }),
                        },
                    ))
                    .unwrap_or_else(|_| {
                        SpawnOutcome::Failed(EngineError::SidecarUnavailable {
                            reason: "engine spawn panicked".to_owned(),
                        })
                    });
                let _ = spawn_tx.send(outcome);
            });
    }

    /// Drena os resultados de spawn do canal (por frame).
    fn drain_spawn_outcomes(&mut self, errors: &mut Vec<String>) {
        let mut ready = false;
        let mut needs_retry = false;
        while let Ok(outcome) = self.spawn_rx.try_recv() {
            match outcome {
                SpawnOutcome::Started(engine, session) => {
                    self.spawn_status = EngineSpawnStatus::Dormant;
                    if self.pending_session != session {
                        // Sessão nova chegou durante o spawn: descarta o
                        // engine velho; a próxima faixa DRM responde com o
                        // spawn da sessão instalada.
                        self.spawned_session = None;
                        drop(engine);
                        needs_retry = true;
                        continue;
                    }
                    self.spawned_session = session;
                    self.engine = Some(engine);
                    ready = true;
                }
                SpawnOutcome::Failed(error) => {
                    self.spawn_status = EngineSpawnStatus::Failed(error.to_string());
                    // Só fala com o usuário quando havia faixa DRM esperando
                    // (falha de spawn destrói a tentativa corrente).
                    if self.current_row_needs_engine() {
                        self.report_error(error.to_string(), errors);
                        self.sync_status(PlaybackStatus::Stopped);
                    }
                }
            }
        }
        if ready || needs_retry {
            // Faixa DRM estava esperando o spawn: retoma.
            self.retry_pending_engine_track();
        }
    }

    /// A faixa corrente não tem fonte nativa (exige engine DRM)?
    fn current_row_needs_engine(&self) -> bool {
        self.current()
            .is_some_and(|row| native_source(row).is_none())
    }

    /// Retoma a faixa corrente quando ela estava esperando o spawn.
    fn retry_pending_engine_track(&mut self) {
        if self.state.status() != PlaybackStatus::Loading {
            return;
        }
        let Some(row) = self.current().cloned() else {
            return;
        };
        if native_source(&row).is_some() {
            return;
        }
        self.loaded_id = None;
        self.start_current();
    }

    /// Drena eventos do backend ativo e devolve `true` quando uma faixa
    /// terminou (a UI avança a fila).
    pub fn drain_player_events(&mut self, errors: &mut Vec<String>) -> bool {
        let mut ended = false;
        // Erros acumulados por caminhos síncronos (sem vetor na mão).
        let pending = std::mem::take(&mut self.pending_errors);
        errors.extend(pending);
        self.drain_spawn_outcomes(errors);
        loop {
            let event = match self.active_backend {
                ActiveBackend::Native => match self.native.events().try_recv() {
                    Ok(event) => Some(BackendEvent::Native(event)),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.report_backend_disconnect(errors);
                        break;
                    }
                },
                ActiveBackend::Engine => match self
                    .engine
                    .as_ref()
                    .map(|engine| engine.events().try_recv())
                {
                    Some(Ok(event)) => Some(BackendEvent::Engine(event)),
                    Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => None,
                    Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                        self.report_backend_disconnect(errors);
                        break;
                    }
                },
            };
            let Some(event) = event else { break };
            match event {
                BackendEvent::Native(PlayerEvent::StateChanged(state)) => {
                    let next = match state {
                        sitrus_player::PlayerState::Playing => PlaybackStatus::Playing,
                        sitrus_player::PlayerState::Paused => PlaybackStatus::Paused,
                        sitrus_player::PlayerState::Stopped => PlaybackStatus::Stopped,
                    };
                    self.sync_status(next);
                }
                BackendEvent::Native(PlayerEvent::Position(position)) => {
                    self.position_secs = secs_from_duration(position);
                }
                BackendEvent::Native(PlayerEvent::Duration(duration)) => {
                    self.state.set_duration(Some(duration));
                }
                BackendEvent::Native(PlayerEvent::Ended)
                | BackendEvent::Engine(EngineEvent::Ended) => {
                    ended = true;
                }
                BackendEvent::Native(PlayerEvent::Error(message)) => {
                    self.report_error(message, errors);
                }
                BackendEvent::Engine(EngineEvent::Error(message)) => {
                    // Erro reportado pela página/renderer (sem fila, CDM etc.).
                    self.report_error(message, errors);
                }
                BackendEvent::Engine(EngineEvent::CommandFailed(error)) => {
                    // Resultado tipado de um comando enfileirado (m3): a
                    // classificação vem da variante, não de string.
                    self.engine_command_failed(error);
                }
                BackendEvent::Engine(EngineEvent::StateChanged(state)) => {
                    self.sync_status(match state {
                        EngineState::Stopped => PlaybackStatus::Stopped,
                        EngineState::Loading => PlaybackStatus::Loading,
                        EngineState::Playing => PlaybackStatus::Playing,
                        EngineState::Paused => PlaybackStatus::Paused,
                    });
                }
                BackendEvent::Engine(EngineEvent::Position(position)) => {
                    self.position_secs = secs_from_duration(position);
                }
                BackendEvent::Engine(EngineEvent::Duration(duration)) => {
                    self.state.set_duration(Some(duration));
                }
            }
        }
        ended
    }

    fn report_backend_disconnect(&mut self, errors: &mut Vec<String>) {
        if self.active_backend == ActiveBackend::Engine {
            self.engine_command_failed(EngineError::ConnectionLost);
        } else {
            self.report_error("backend de reprodução saiu do ar".to_owned(), errors);
        }
    }

    fn report_error(&mut self, message: String, errors: &mut Vec<String>) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            self.last_error = Some(message.clone());
            errors.push(message);
        }
    }

    /// Erro visível em caminho síncrono (sem vetor de erros na mão):
    /// acumulado e entregue no próximo drain (toast da UI, M3).
    fn defer_error(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            self.last_error = Some(message.clone());
            self.pending_errors.push(message);
        }
    }

    fn sync_status(&mut self, next: PlaybackStatus) {
        if self.state.status() == PlaybackStatus::Stopped && next == PlaybackStatus::Playing {
            let _ = self.state.transition_to(PlaybackStatus::Loading);
        }
        let _ = self.state.transition_to(next);
    }

    /// Reconstrói a fila a partir de uma coleção visível e seleciona a faixa
    /// clicada (comportamento "tocar a partir daqui" do Apple Music).
    pub fn play_collection(&mut self, rows: &[SongRow], selected: usize) {
        self.stop_playback();
        let (queue, queue_items) = build_queue(rows, selected);
        self.queue = queue;
        self.queue_items = queue_items;
        self.rebuild_title();
        self.start_current();
    }

    /// Alterna play/pause da faixa atual.
    pub fn toggle_play(&mut self) {
        if self.queue.current().is_none() {
            return;
        }
        if self.state.status() == PlaybackStatus::Playing {
            self.pause_backend();
        } else {
            self.play_backend();
        }
    }

    /// Pula para uma faixa específica e inicia sua reprodução.
    pub fn move_to(&mut self, index: usize) -> bool {
        if self.queue.move_to(index).is_none() {
            return false;
        }
        self.current_index = self.queue.current_index();
        self.rebuild_title();
        self.loaded_id = None;
        self.start_current();
        true
    }

    /// Reordena a faixa arrastada sem perder a faixa corrente.
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if self.queue.reorder(from, to).is_none() {
            return false;
        }
        let row = self.queue_items.remove(from);
        self.queue_items.insert(to, row);
        self.current_index = self.queue.current_index();
        true
    }

    /// Faixa anterior (sem repetir no início).
    pub fn previous(&mut self) {
        if self.queue.previous().is_some() {
            self.current_index = self.queue.current_index();
            self.rebuild_title();
            self.reload_and_play();
        }
    }

    /// Próxima faixa (usado pelo botão, atalho e auto-avanço no fim).
    pub fn advance(&mut self) {
        if self.queue.next().is_some() {
            self.current_index = self.queue.current_index();
            self.rebuild_title();
            self.reload_and_play();
        } else if !self.queue.is_empty() {
            self.stop_active_backend();
            self.sync_status(PlaybackStatus::Stopped);
            self.position_secs = 0;
        }
    }

    /// Seek proporcional (0..1) quando há duração conhecida.
    pub fn seek_fraction(&mut self, fraction: f32) {
        if let Some(duration) = self.state.duration() {
            let clamped = fraction.clamp(0.0, 1.0);
            let seconds = (clamped * duration.as_secs_f32()).max(0.0);
            self.seek_backend(Duration::from_secs_f32(seconds));
            self.position_secs = seconds.round().clamp(0.0, u32::MAX as f32) as u32;
        }
    }

    /// Seek por deslocamento de segundos (atalhos).
    pub fn seek_relative(&mut self, delta_secs: i64) {
        let Some(duration) = self.state.duration() else {
            return;
        };
        let current = i64::from(self.position_secs);
        let target = (current + delta_secs).clamp(0, duration.as_secs() as i64) as u64;
        self.seek_backend(Duration::from_secs(target));
        self.position_secs = u32::try_from(target).unwrap_or(u32::MAX);
    }

    /// Volume 0..=1.
    pub fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.state.set_volume(clamped);
        match self.active_backend {
            ActiveBackend::Native => {
                if let Err(error) = self.native.set_volume(clamped) {
                    self.last_error = Some(error.to_string());
                }
            }
            ActiveBackend::Engine => {
                let result = self
                    .engine
                    .as_ref()
                    .map_or(Ok(()), |engine| engine.set_volume(clamped));
                if let Err(error) = result {
                    self.engine_command_failed(error);
                }
            }
        }
    }

    /// Volume corrente (0..=1).
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.state.volume()
    }

    /// Posição corrente (segundos).
    #[must_use]
    pub fn position_secs(&self) -> u32 {
        self.position_secs
    }

    /// Duração conhecida (segundos; 0 quando desconhecida).
    #[must_use]
    pub fn duration_secs(&self) -> u32 {
        self.state.duration().map_or(0, secs_from_duration)
    }

    /// Estamos em reprodução?
    #[must_use]
    pub fn playing(&self) -> bool {
        self.state.status() == PlaybackStatus::Playing
    }

    /// Título/legenda/artista atual para a barra.
    #[must_use]
    pub fn display(&self) -> (&str, &str, Option<&str>) {
        (&self.title, &self.subtitle, self.current_key.as_deref())
    }

    /// A Engine DRM foi iniciada e pode aceitar faixas sem URL direta.
    #[must_use]
    pub fn engine_available(&self) -> bool {
        self.engine.is_some()
    }

    /// Faixa atual (para artwork da barra).
    #[must_use]
    pub fn current(&self) -> Option<&SongRow> {
        self.current_index.and_then(|i| self.queue_items.get(i))
    }

    /// Itens da fila (espelho de exibição).
    #[must_use]
    pub fn queue_items(&self) -> &[SongRow] {
        &self.queue_items
    }

    /// Índice ativo no painel de fila.
    #[must_use]
    pub const fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    /// A fila mudou desde a faixa carregada (para o painel).
    #[must_use]
    pub fn has_queue(&self) -> bool {
        !self.queue_items.is_empty()
    }

    /// Carrega a faixa corrente no backend apropriado: prévias/arquivos no
    /// player nativo, itens sem URL na Engine DRM.
    fn start_current(&mut self) {
        let Some(index) = self.queue.current_index() else {
            return;
        };
        let Some(row) = self.queue_items.get(index).cloned() else {
            return;
        };
        self.current_index = Some(index);
        self.current_key = Some(row.id.clone());
        self.position_secs = 0;
        self.state.set_duration(None);
        self.sync_status(PlaybackStatus::Loading);
        if let Some(source) = native_source(&row) {
            self.active_backend = ActiveBackend::Native;
            self.loaded_id = Some(row.id.clone());
            if let Err(error) = self.native.load(source) {
                self.last_error = Some(error.to_string());
                return;
            }
            let _ = self.native.set_volume(self.state.volume());
            if let Err(error) = self.native.play() {
                self.last_error = Some(error.to_string());
            }
            return;
        }

        let Some(engine) = self.engine.as_ref() else {
            self.active_backend = ActiveBackend::Native;
            self.loaded_id = None;
            // Spawn preguiçoso (M2): a primeira faixa sem URL/prévia manda
            // subir o sidecar em thread de fundo; a UI continua interativa
            // (status Loading) e o resultado chega pelo drain por frame —
            // sucesso retoma a faixa, falha vira aviso visível.
            self.begin_spawn();
            return;
        };
        self.active_backend = ActiveBackend::Engine;
        self.loaded_id = None;
        let queue_item = QueueItem {
            id: row.id.clone(),
            title: Some(row.title.clone()),
            duration_ms: (row.duration_secs > 0).then(|| u64::from(row.duration_secs) * 1000),
            url: None,
        };
        let result = engine
            .load_queue(vec![queue_item])
            .and_then(|()| engine.set_volume(self.state.volume()))
            .and_then(|()| engine.play());
        if let Err(error) = result {
            self.engine_command_failed(error);
        }
    }

    fn play_backend(&mut self) {
        match self.active_backend {
            ActiveBackend::Native => {
                if let Err(error) = self.native.play() {
                    self.last_error = Some(error.to_string());
                }
            }
            ActiveBackend::Engine => {
                let result = self.engine.as_ref().map_or(Ok(()), |engine| engine.play());
                if let Err(error) = result {
                    self.engine_command_failed(error);
                }
            }
        }
    }

    fn pause_backend(&mut self) {
        match self.active_backend {
            ActiveBackend::Native => {
                if let Err(error) = self.native.pause() {
                    self.last_error = Some(error.to_string());
                }
            }
            ActiveBackend::Engine => {
                let result = self.engine.as_ref().map_or(Ok(()), |engine| engine.pause());
                if let Err(error) = result {
                    self.engine_command_failed(error);
                }
            }
        }
    }

    fn seek_backend(&mut self, position: Duration) {
        match self.active_backend {
            ActiveBackend::Native => {
                if let Err(error) = self.native.seek(position) {
                    self.last_error = Some(error.to_string());
                }
            }
            ActiveBackend::Engine => {
                let result = self
                    .engine
                    .as_ref()
                    .map_or(Ok(()), |engine| engine.seek(position));
                if let Err(error) = result {
                    self.engine_command_failed(error);
                }
            }
        }
    }

    /// Erro tipado de comando enfileirado (m3/M3): erros de RPC
    /// (reprodução/licenciamento/página) ficam VISÍVEIS na UI sem trocar de
    /// backend; erros que pedem fallback nativo derrubam o engine e tentam
    /// manter a faixa corrente no player nativo.
    fn engine_command_failed(&mut self, error: EngineError) {
        if !error.requires_native_fallback() {
            self.defer_error(error.to_string());
            return;
        }
        self.engine = None;
        self.spawn_status = EngineSpawnStatus::Dormant;
        self.active_backend = ActiveBackend::Native;
        let Some(row) = self.current().cloned() else {
            self.defer_error(error.to_string());
            return;
        };
        let Some(source) = native_source(&row) else {
            // Faixa DRM sem fonte nativa: sem fallback possível, aviso.
            self.defer_error(format!(
                "{}; esta faixa não tem prévia nem arquivo local para fallback",
                error
            ));
            return;
        };
        if let Err(native_error) = self.native.load(source) {
            self.defer_error(native_error.to_string());
            return;
        }
        let _ = self.native.set_volume(self.state.volume());
        let result = match self.state.status() {
            PlaybackStatus::Loading => self.native.play(),
            PlaybackStatus::Playing => self.native.play(),
            PlaybackStatus::Paused => {
                let played = self.native.play();
                played.and_then(|()| self.native.pause())
            }
            PlaybackStatus::Stopped | PlaybackStatus::Error => Ok(()),
        };
        if let Err(native_error) = result {
            self.defer_error(native_error.to_string());
        }
    }

    fn stop_active_backend(&mut self) {
        match self.active_backend {
            ActiveBackend::Native => {
                let _ = self.native.stop();
            }
            ActiveBackend::Engine => {
                if let Some(engine) = &self.engine
                    && let Err(error) = engine.load_queue(Vec::new())
                {
                    self.last_error = Some(error.to_string());
                }
            }
        }
        self.active_backend = ActiveBackend::Native;
    }

    /// Recarrega a faixa corrente após next/previous.
    fn reload_and_play(&mut self) {
        self.loaded_id = None;
        self.start_current();
    }

    /// Reinicia título/legenda a partir da faixa corrente.
    fn rebuild_title(&mut self) {
        match self.queue.current() {
            Some(song) => {
                self.title = song.display_title().to_owned();
                self.subtitle = song.album.as_str().to_owned();
            }
            None => {
                self.title.clear();
                self.subtitle.clear();
            }
        }
        self.current_index = self.queue.current_index();
        self.last_error = None;
    }

    /// Interrompe a reprodução e zera a fila de exibição.
    fn stop_playback(&mut self) {
        self.stop_active_backend();
        self.sync_status(PlaybackStatus::Stopped);
        self.position_secs = 0;
        self.state.set_duration(None);
        self.loaded_id = None;
        self.current_key = None;
        self.current_index = None;
        self.title.clear();
        self.subtitle.clear();
    }

    /// Encerra a reprodução e esvazia a fila de exibição (logout).
    pub fn clear(&mut self) {
        self.stop_playback();
        self.queue = PlaybackQueue::new();
        self.queue_items.clear();
    }
}

impl Default for EngineController {
    fn default() -> Self {
        Self::new()
    }
}

/// Compatibilidade com o nome já consumido pelo app.
pub type PlayerController = EngineController;

#[derive(Debug)]
enum BackendEvent {
    Native(PlayerEvent),
    Engine(EngineEvent),
}

fn native_source(row: &SongRow) -> Option<String> {
    if let Some(url) = row.preview_url.as_ref() {
        return Some(url.as_str().to_owned());
    }
    Path::new(&row.id).is_file().then(|| row.id.clone())
}

/// Spawner real do sidecar (thread de fundo chama; nunca o thread da UI).
fn sidecar_spawner() -> EngineSpawner {
    Arc::new(|config: &SidecarConfig| {
        SidecarEngine::spawn(config).map(|engine| Box::new(engine) as Box<dyn Engine + Send>)
    })
}

/// `SITRUS_ENGINE_WINDOW=1` mostra a janela do Electron do engine, para
/// inspecionar a página real durante a validação. O padrão é oculta: a janela
/// existe só para hospedar o CDM, não é interface do app.
fn engine_window_visible() -> bool {
    std::env::var("SITRUS_ENGINE_WINDOW").is_ok_and(|value| value == "1")
}

fn engine_directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SITRUS_ENGINE_DIR").map(PathBuf::from)
        && path.join("sidecar.js").is_file()
    {
        return Some(path);
    }
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine"),
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("../lib/sitrus/engine"),
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("../libexec/sitrus/engine"),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("sidecar.js").is_file())
}

/// Monta a fila pura a partir de uma coleção visível (ex.: faixas de um
/// álbum/playlist/artista) e posiciona o cursor na faixa `selected`
/// (comportamento "tocar a partir daqui"). O espelho de exibição acompanha a
/// ordem da coleção. Sem dependência do backend de áudio — testável offline.
#[must_use]
pub fn build_queue(rows: &[SongRow], selected: usize) -> (PlaybackQueue, Vec<SongRow>) {
    let mut queue = PlaybackQueue::new();
    let mut queue_items: Vec<SongRow> = Vec::with_capacity(rows.len());
    for row in rows {
        queue.add_last(row.song.clone());
        queue_items.push(row.clone());
    }
    let target = selected.min(queue.len().saturating_sub(1));
    for _ in 0..=target {
        queue.next();
    }
    (queue, queue_items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::secs_from_millis;
    use sitrus_core::model::{EntityId, Song};
    use sitrus_engine::SidecarCookie;
    use std::time::Duration as StdDuration;
    use url::Url;

    /// Linha de faixa de álbum com prévia tocável (catálogo).
    fn album_track(id: &str, title: &str, preview: bool) -> SongRow {
        let preview_url = preview.then(|| {
            Url::parse(&format!("https://preview.example/{id}.mp3")).expect("url de teste")
        });
        let duration_millis: u64 = 210_000;
        SongRow {
            song: Song {
                id: EntityId::new(id),
                title: title.to_owned(),
                album: EntityId::new("album-9"),
                duration: Some(StdDuration::from_millis(duration_millis)),
                preview_url: preview_url.clone(),
            },
            id: id.to_owned(),
            title: title.to_owned(),
            subtitle: "Artista".to_owned(),
            album_name: "Álbum".to_owned(),
            duration_secs: secs_from_millis(duration_millis),
            artwork_url: None,
            preview_url,
            play_count: None,
        }
    }

    #[test]
    fn engine_controller_routes_drm_tracks_and_moves_queue_cursor() {
        use sitrus_engine::MockEngine;

        let engine = MockEngine::with_position_interval(Duration::from_millis(5));
        let mut controller = EngineController::with_engine(Some(Box::new(engine)));
        let rows = vec![album_track("a", "A", false), album_track("b", "B", false)];
        controller.play_collection(&rows, 0);
        assert_eq!(controller.active_backend, ActiveBackend::Engine);
        assert!(controller.move_to(1));
        assert_eq!(controller.current_index(), Some(1));
        assert_eq!(controller.current().map(|row| row.id.as_str()), Some("b"));

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !controller.playing() && std::time::Instant::now() < deadline {
            controller.drain_player_events(&mut Vec::new());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            controller.playing(),
            "Engine state event should reach player bar"
        );
        controller.seek_fraction(0.5);
        let seek_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while controller.position_secs() < 100 && std::time::Instant::now() < seek_deadline {
            controller.drain_player_events(&mut Vec::new());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            controller.position_secs() >= 100,
            "seek event should update the bar"
        );
        controller.toggle_play();
        let pause_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while controller.state.status() != PlaybackStatus::Paused
            && std::time::Instant::now() < pause_deadline
        {
            controller.drain_player_events(&mut Vec::new());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(controller.state.status(), PlaybackStatus::Paused);
        controller.set_volume(0.4);
        assert!((controller.volume() - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn queue_drag_reorder_keeps_current_track_and_moves_display_rows() {
        let mut controller = EngineController::with_engine(None);
        let rows = vec![
            album_track("a", "A", true),
            album_track("b", "B", true),
            album_track("c", "C", true),
        ];
        controller.play_collection(&rows, 1);
        assert!(controller.reorder(0, 2));
        assert_eq!(controller.current_index(), Some(0));
        assert_eq!(controller.current().map(|row| row.id.as_str()), Some("b"));
        assert_eq!(
            controller
                .queue_items()
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        assert!(controller.move_to(2));
        assert_eq!(controller.current().map(|row| row.id.as_str()), Some("a"));
    }

    #[test]
    fn native_player_is_selected_for_preview_when_engine_is_unavailable() {
        let mut controller = EngineController::with_engine(None);
        controller.play_collection(&[album_track("preview", "Preview", true)], 0);
        assert_eq!(controller.active_backend, ActiveBackend::Native);
        assert_eq!(controller.loaded_id.as_deref(), Some("preview"));
    }

    /// Fila de álbum: ordem fiel, tamanho correto e cursor na faixa clicada.
    #[test]
    fn queue_from_album_selects_clicked_track() {
        let rows = vec![
            album_track("t1", "Abertura", true),
            album_track("t2", "Meio", true),
            album_track("t3", "Fim", true),
        ];
        let (queue, items) = build_queue(&rows, 1);
        assert_eq!(items.len(), 3, "espelho da fila com as três faixas");
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.len(), items.len());
        let current = queue.current().expect("faixa posicionada");
        assert_eq!(current.id.as_str(), "t2", "cursor na faixa clicada");
        assert_eq!(current.title, "Meio");
        // Ordem preservada no espelho de exibição.
        assert_eq!(items[0].id, "t1");
        assert_eq!(items[1].id, "t2");
        assert_eq!(items[2].id, "t3");
        // Prévias preservadas (reprodução real da próxima faixa).
        assert!(rows.iter().all(|row| row.preview_url.is_some()));
    }

    /// Clique na última faixa: cursor na última posição.
    #[test]
    fn queue_positions_last_track() {
        let rows = vec![album_track("t1", "A", true), album_track("t2", "B", true)];
        let (queue, _) = build_queue(&rows, 1);
        assert_eq!(
            queue.current().map(|song| song.id.as_str().to_owned()),
            Some("t2".to_owned())
        );
    }

    /// Seleção fora da faixa (por fila encurtada, ex.) cai na última.
    #[test]
    fn queue_clamps_out_of_range_selection() {
        let rows = vec![album_track("t1", "Só", true)];
        let (queue, _) = build_queue(&rows, 9);
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.current().map(|song| song.id.as_str().to_owned()),
            Some("t1".to_owned())
        );
    }

    /// Coleção vazia: nenhuma faixa, cursor indefinido.
    #[test]
    fn queue_from_empty_collection_is_empty() {
        let (queue, items) = build_queue(&[], 0);
        assert!(queue.is_empty());
        assert!(items.is_empty());
        assert!(queue.current().is_none());
    }

    /// M2: o construtor NÃO sobe engine — nenhum sidecar no arranque do app.
    #[test]
    fn engine_controller_new_does_not_spawn_the_sidecar_or_anything() {
        let mut controller = EngineController::new();
        assert!(!controller.engine_available(), "nenhum engine no arranque");
        assert_eq!(controller.spawn_status, EngineSpawnStatus::Dormant);
        // Um drain (frame) não dispara spawn nem erro: nada aconteceu.
        let mut errors = Vec::new();
        let _ = controller.drain_player_events(&mut errors);
        assert!(errors.is_empty());
        assert!(!controller.engine_available());
        assert_eq!(controller.spawn_status, EngineSpawnStatus::Dormant);
    }

    /// Sessão instalada → primeira faixa DRM → spawn em thread de fundo, com
    /// a sessão chegando INTEGRA ao `SidecarConfig` (cookies + media token) e
    /// NENHUM segredo em qualquer stringificação de log (Debug é redigido).
    #[test]
    fn first_drm_track_spawns_engine_with_the_installed_session() {
        const MEDIA_TOKEN: &str = "SITRUS-APP-MEDIA-TOKEN-77c1";
        const COOKIE_NAME: &str = "citm";
        const COOKIE_VALUE: &str = "SITRUS-APP-COOKIE-9f3a2b";

        let captured: Arc<std::sync::Mutex<Option<SidecarConfig>>> =
            Arc::new(std::sync::Mutex::new(None));
        let capture_for_spawner = Arc::clone(&captured);
        let spawner: EngineSpawner = Arc::new(move |config: &SidecarConfig| {
            *capture_for_spawner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(config.clone());
            Ok(Box::new(sitrus_engine::MockEngine::with_position_interval(
                Duration::from_millis(5),
            )) as Box<dyn Engine + Send>)
        });

        let mut controller = EngineController::with_spawner(spawner);
        controller.set_user_session(Some(SidecarSession {
            media_user_token: Some(MEDIA_TOKEN.to_owned()),
            cookies: vec![SidecarCookie {
                name: COOKIE_NAME.to_owned(),
                value: COOKIE_VALUE.to_owned(),
                domain: None,
            }],
        }));

        // Faixa sem prévia nem arquivo local: exige engine DRM.
        controller.play_collection(&[album_track("drm", "D", false)], 0);

        // O spawn roda em thread de fundo; a UI continua interativa e drena
        // por frame até o engine ficar pronto e a faixa tocar.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while (!controller.engine_available()
            || controller.state.status() != PlaybackStatus::Playing)
            && std::time::Instant::now() < deadline
        {
            let _ = controller.drain_player_events(&mut Vec::new());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            controller.engine_available(),
            "lazy spawn must finish in the background and become available"
        );
        assert_eq!(
            controller.state.status(),
            PlaybackStatus::Playing,
            "the DRM track resumes automatically after the spawn"
        );

        // O envelope chegou INTEGRO ao SidecarConfig consumido pelo spawn.
        let config = captured
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("test spawner captured the SidecarConfig");
        let session = config.session.clone().expect("session wired into config");
        assert_eq!(session.media_user_token.as_deref(), Some(MEDIA_TOKEN));
        assert_eq!(session.cookies.len(), 1);
        assert_eq!(session.cookies[0].name, COOKIE_NAME);
        assert_eq!(session.cookies[0].value, COOKIE_VALUE);
        assert_eq!(session.cookies[0].domain, None);
        let envelope = session.envelope();
        assert!(
            envelope.contains(MEDIA_TOKEN) && envelope.contains(COOKIE_VALUE),
            "envelope transports media token and cookie: {envelope}"
        );
        // E NADA de segredo em qualquer Debug/log da configuração.
        let rendered = format!("{config:?}");
        assert!(
            !rendered.contains(MEDIA_TOKEN) && !rendered.contains(COOKIE_VALUE),
            "config debug must stay secret-free: {rendered}"
        );

        // Sessão nova diverge da sessão do spawn: o engine é descartado
        // (Drop não bloqueante) e a próxima faixa DRM responde com a nova.
        controller.set_user_session(Some(SidecarSession {
            media_user_token: Some("other-token".to_owned()),
            ..SidecarSession::default()
        }));
        assert!(
            !controller.engine_available(),
            "engine spawned with the old session must be dropped on session change"
        );
    }

    /// Falha de spawn vira aviso visível (erro entra no dreno da UI/toast),
    /// e a faixa DRM sai do estado Loading.
    #[test]
    fn failed_engine_spawn_becomes_a_visible_error() {
        let spawner: EngineSpawner = Arc::new(
            |_config: &SidecarConfig| -> Result<Box<dyn Engine + Send>, EngineError> {
                Err(EngineError::SidecarUnavailable {
                    reason: "node ausente".to_owned(),
                })
            },
        );
        let mut controller = EngineController::with_spawner(spawner);
        controller.play_collection(&[album_track("drm", "D", false)], 0);
        assert!(!controller.engine_available(), "engine still not ready");

        // Drena por "frame" acumulando os erros da UI.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut errors = Vec::new();
        while errors.is_empty() && std::time::Instant::now() < deadline {
            controller.drain_player_events(&mut errors);
            if errors.is_empty() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        controller.drain_player_events(&mut errors);
        assert!(
            errors
                .iter()
                .any(|message| message.contains("node ausente")),
            "spawn failure must surface to the UI: {errors:?}"
        );
        assert_eq!(
            controller.engine_spawn_message().as_deref(),
            Some("sidecar is unavailable: node ausente"),
            "UI warning carries the typed spawn failure"
        );
        assert_eq!(
            controller.state.status(),
            PlaybackStatus::Stopped,
            "the DRM track is no longer stuck in Loading"
        );
    }
}
