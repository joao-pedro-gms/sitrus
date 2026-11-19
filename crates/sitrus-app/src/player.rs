//! Controlador do player: cola a barra de rodapé com `sitrus-player`
//! (comandos/eventos) e a fila de reprodução de `sitrus-core` com
//! `PlayerState`. Nenhuma operação bloqueia a thread da UI — o backend de
//! áudio roda na thread dele e os eventos são drenados a cada frame.

use std::time::Duration;

use sitrus_core::player_state::{PlaybackStatus, PlayerState as CorePlayerState};
use sitrus_core::queue::PlaybackQueue;
use sitrus_player::{Player, PlayerEvent};

use crate::model::{SongRow, secs_from_duration};

/// Controlador de transporte + fila.
pub struct PlayerController {
    player: Player,
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
}

impl PlayerController {
    /// Cria o controlador e inicia o backend de áudio (thread própria).
    #[must_use]
    pub fn new() -> Self {
        Self {
            player: Player::new(),
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
        }
    }

    /// Drena os eventos do backend (não bloqueia) e devolve erros novos
    /// para a UI mostrar uma vez. Devolve `true` quando uma faixa terminou
    /// (a UI decide avançar automaticamente).
    pub fn drain_player_events(&mut self, errors: &mut Vec<String>) -> bool {
        let mut ended = false;
        loop {
            match self.player.events().try_recv() {
                Ok(event) => match event {
                    PlayerEvent::StateChanged(state) => {
                        let next = match state {
                            sitrus_player::PlayerState::Playing => PlaybackStatus::Playing,
                            sitrus_player::PlayerState::Paused => PlaybackStatus::Paused,
                            sitrus_player::PlayerState::Stopped => PlaybackStatus::Stopped,
                        };
                        // Transição inválida não é fatal (ex.: stop duplo);
                        // o estado de UI acompanha o backend de qualquer forma.
                        let _ = self.state.transition_to(next);
                    }
                    PlayerEvent::Position(position) => {
                        self.position_secs = secs_from_duration(position);
                    }
                    PlayerEvent::Duration(duration) => {
                        self.state.set_duration(Some(duration));
                    }
                    PlayerEvent::Ended => ended = true,
                    PlayerEvent::Error(message) => {
                        if self.last_error.as_deref() != Some(message.as_str()) {
                            self.last_error = Some(message.clone());
                            errors.push(message);
                        }
                    }
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Backend morto: reporta uma vez e para de drenar.
                    if self.last_error.is_none() {
                        self.last_error = Some("backend de áudio saiu do ar".to_owned());
                        errors.push("backend de áudio saiu do ar".to_owned());
                    }
                    break;
                }
            }
        }
        ended
    }

    /// Reconstrói a fila a partir de uma coleção visível e seleciona a faixa
    /// clicada (comportamento "tocar a partir daqui" do Apple Music).
    pub fn play_collection(&mut self, rows: &[SongRow], selected: usize) {
        self.stop_playback();
        self.queue = PlaybackQueue::new();
        self.queue_items = Vec::new();
        let mut selected_key = None;
        for (index, row) in rows.iter().enumerate() {
            self.queue.add_last(row.song.clone());
            self.queue_items.push(row.clone());
            if index == selected {
                selected_key = Some(row.id.clone());
            }
        }
        let target = selected.min(self.queue.len().saturating_sub(1));
        for _ in 0..=target {
            self.queue.next();
        }
        self.rebuild_title();
        self.start_current();
        debug_assert_eq!(selected_key, self.current_key, "fila selecionou outro item");
    }

    /// Alterna play/pause da faixa atual.
    pub fn toggle_play(&mut self) {
        if self.queue.current().is_none() {
            return;
        }
        if self.state.status() == PlaybackStatus::Playing {
            let _ = self.player.pause();
        } else {
            let _ = self.player.play();
        }
    }

    /// Faixa anterior (sem repetir no início).
    pub fn previous(&mut self) {
        if self.queue.previous().is_some() {
            self.rebuild_title();
            self.reload_and_play();
        }
    }

    /// Próxima faixa (usado pelo botão, atalho e auto-avanço no fim).
    pub fn advance(&mut self) {
        if self.queue.next().is_some() {
            self.rebuild_title();
            self.reload_and_play();
        } else if !self.queue.is_empty() {
            // Fim da fila: para no limite.
            let _ = self.player.stop();
            let _ = self.state.transition_to(PlaybackStatus::Stopped);
            self.position_secs = 0;
        }
    }

    /// Seek proporcional (0..1) quando há duração conhecida.
    pub fn seek_fraction(&mut self, fraction: f32) {
        if let Some(duration) = self.state.duration() {
            let clamped = fraction.clamp(0.0, 1.0);
            let seconds = (clamped * duration.as_secs_f32()).max(0.0);
            let _ = self.player.seek(Duration::from_secs_f32(seconds));
            self.position_secs = seconds.round().clamp(0.0, u32::MAX as f32) as u32;
        }
    }

    /// Seek por deslocamento de segundos (atalhos).
    pub fn seek_relative(&mut self, delta_secs: i64) {
        let Some(duration) = self.state.duration() else {
            return;
        };
        let current = i64::from(self.position_secs);
        let target = (current + delta_secs).clamp(0, duration.as_secs() as i64);
        let target = target as u64;
        let _ = self.player.seek(Duration::from_secs(target));
        self.position_secs = u32::try_from(target).unwrap_or(u32::MAX);
    }

    /// Volume 0..=1.
    pub fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.state.set_volume(clamped);
        let _ = self.player.set_volume(clamped);
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

    /// Reproduz a faixa corrente da fila, se ela tiver URL de reprodução.
    ///
    /// A biblioteca da Apple (`/v1/me/library/songs`) não expõe URLs de
    /// stream: nessas faixes a UI mostra aviso, e o mesmo acontece se o
    /// backend devolver erro (saída de áudio, decodificação etc.).
    fn start_current(&mut self) {
        let Some(current) = self.queue.current().cloned() else {
            return;
        };
        self.current_key = Some(current.id.as_str().to_owned());
        self.current_index = self
            .queue_items
            .iter()
            .position(|row| row.id.as_str() == current.id.as_str());
        if self.loaded_id.as_deref() == Some(current.id.as_str()) {
            // Mesma faixa: apenas garante reprodução.
            let _ = self.player.play();
            return;
        }
        match current.preview_url {
            Some(url) => {
                self.loaded_id = Some(current.id.as_str().to_owned());
                self.position_secs = 0;
                self.state.set_duration(None);
                if let Err(error) = self.player.load(url.as_str()) {
                    self.last_error = Some(error.to_string());
                } else if let Err(error) = self.player.play() {
                    self.last_error = Some(error.to_string());
                }
            }
            None => {
                self.loaded_id = None;
                self.state.set_duration(None);
                self.position_secs = 0;
                let _ = self.state.transition_to(PlaybackStatus::Stopped);
            }
        }
    }

    /// Recarrega a faixa corrente após next/previous.
    fn reload_and_play(&mut self) {
        // next/previous pode voltar a uma faixa já carregada (id igual):
        // nesse caso reinicia do zero em vez de continuar do fim.
        if self
            .queue
            .current()
            .is_some_and(|song| song.id.as_str() == self.loaded_id.as_deref().unwrap_or(""))
        {
            let _ = self.player.seek(Duration::ZERO);
            self.position_secs = 0;
            let _ = self.player.play();
            return;
        }
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
                self.title = String::new();
                self.subtitle = String::new();
            }
        }
        self.last_error = None;
    }

    /// Interrompe a reprodução e zera a fila de exibição.
    fn stop_playback(&mut self) {
        let _ = self.player.stop();
        let _ = self.state.transition_to(PlaybackStatus::Stopped);
        self.position_secs = 0;
        self.state.set_duration(None);
        self.loaded_id = None;
        self.current_key = None;
        self.current_index = None;
        self.title = String::new();
        self.subtitle = String::new();
    }

    /// Encerra a reprodução e esvazia a fila de exibição (logout).
    pub fn clear(&mut self) {
        self.stop_playback();
        self.queue_items.clear();
    }
}

impl Default for PlayerController {
    fn default() -> Self {
        Self::new()
    }
}
