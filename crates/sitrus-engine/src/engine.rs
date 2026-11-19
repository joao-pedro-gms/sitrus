//! Contrato do engine: trait [`Engine`], estado, eventos e itens de fila.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::error::EngineError;

/// Limite superior inclusivo de volume.
pub const VOLUME_MAX: f32 = 1.0;

/// Estado de reprodução do engine, independente de backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    /// Sem fila carregada ou reprodução interrompida.
    Stopped,
    /// Preparando a fonte (renderer do sidecar ainda não está pronto).
    Loading,
    /// Reprodução em andamento.
    Playing,
    /// Reprodução pausada.
    Paused,
}

/// Evento assíncrono emitido pelo engine ao consumidor.
///
/// O formato espelha `PlayerEvent` do `sitrus-player` para que a integração
/// no app seja direta, mas os tipos são próprios deste crate.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// O estado de reprodução mudou.
    StateChanged(EngineState),
    /// Posição atual da reprodução.
    Position(Duration),
    /// Duração conhecida da faixa carregada.
    Duration(Duration),
    /// A faixa terminou de tocar (o engine avança na fila e aguarda `play`).
    Ended,
    /// Falha reportada pelo backend (sem fila, renderer do lado da página, CDM etc.).
    Error(String),
    /// Falha assíncrona de um comando enfileirado (resultado do RPC que não
    /// tem chamador bloqueante): o erro tipado chega pelo canal de eventos.
    /// Erros de desconexão/de prazo do próprio sidecar também chegam aqui —
    /// [`EngineError::ConnectionLost`] indica que o engine caiu e pede
    /// fallback nativo; [`EngineErrorKind::Rpc`] são falhas de reprodução
    /// reportadas pelo protocolo (visíveis ao usuário, sem fallback).
    CommandFailed(EngineError),
}

/// Entrada da fila de reprodução dirigida ao engine.
///
/// `id` é opaco para o crate: no sidecar é o identificador do catálogo/libraria
/// da Apple Music; `url` é um destino direto opcional (por exemplo, prévia).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueItem {
    /// Identificador opaco da faixa (vazio é rejeitado por [`Engine::load_queue`]).
    pub id: String,
    /// Título de exibição opcional, repassado ao renderer.
    pub title: Option<String>,
    /// Duração conhecida em milissegundos (o renderer reporta a real quando tocar).
    pub duration_ms: Option<u64>,
    /// URL de mídia direta opcional para fontes sem DRM.
    pub url: Option<String>,
}

impl QueueItem {
    /// Cria um item com identificador e campos opcionais zerados.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }

    /// Valida o item: `id` não pode ficar vazio.
    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() {
            return Err(EngineError::InvalidQueueItem {
                reason: "queue item id must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// Contrato comum dos backends de reprodução (nativo ou sidecar DRM).
///
/// Os comandos são **enfileirados sem bloquear o chamador**: o [`Result`] do
/// retorno é de validação local (volume, fila) ou de infraestrutura (canal
/// fechado) — ele **não** espera a resposta do backend. O resultado assíncrono
/// (estado, posição, falhas de reprodução) chega pelo canal de
/// [`Engine::events`]; falhas de comandos enfileirados chegam como
/// [`EngineEvent::CommandFailed`] com o erro tipado (desconexão pede fallback
/// nativo; erros JSON-RPC do protocolo são visíveis ao usuário, sem fallback).
pub trait Engine {
    /// Substitui a fila inteira e volta ao estado [`EngineState::Stopped`]
    /// na posição zero. Fila vazia apenas limpa a atual.
    fn load_queue(&self, queue: Vec<QueueItem>) -> Result<(), EngineError>;

    /// Inicia ou retoma a reprodução do item corrente da fila.
    fn play(&self) -> Result<(), EngineError>;

    /// Pausa a reprodução atual.
    fn pause(&self) -> Result<(), EngineError>;

    /// Solicita a posição indicada, limitada pela duração da faixa carregada.
    fn seek(&self, position: Duration) -> Result<(), EngineError>;

    /// Ajusta o volume no intervalo inclusivo de 0.0 a [`VOLUME_MAX`].
    fn set_volume(&self, volume: f32) -> Result<(), EngineError>;

    /// Último estado conhecido (atualizado por [`EngineEvent::StateChanged`]).
    fn state(&self) -> EngineState;

    /// Canal de eventos assíncronos emitidos pelo backend.
    fn events(&self) -> &Receiver<EngineEvent>;
}

/// Valida o volume contra o intervalo 0.0..=[`VOLUME_MAX`]; NaN é rejeitado.
pub(crate) fn validate_volume(volume: f32) -> Result<(), EngineError> {
    if !(0.0..=VOLUME_MAX).contains(&volume) {
        return Err(EngineError::VolumeOutOfRange(volume));
    }
    Ok(())
}
