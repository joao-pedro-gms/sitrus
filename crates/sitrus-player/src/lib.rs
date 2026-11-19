//! Backend de reprodução nativa do Sitrus.
//!
//! Os comandos são enviados a uma thread dedicada; resultados e progresso são publicados pelo canal
//! de eventos. Carregamento local, requisições HTTP e decodificação não bloqueiam a thread da UI.

mod decode;
#[cfg(feature = "hls")]
mod hls;
mod player;
mod source;

use thiserror::Error;

pub use player::Player;

/// Formatos de arquivo decodificados pelo Symphonia e fontes usadas em HLS.
pub const SUPPORTED_FORMATS: &[&str] = &["aac", "m4a", "mp3", "flac", "wav"];

/// Estado de reprodução informado ao consumidor pelo canal de eventos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// Reprodução interrompida ou ainda não iniciada.
    Stopped,
    /// Reprodução em andamento.
    Playing,
    /// Reprodução pausada.
    Paused,
}

/// Evento assíncrono produzido pelo backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// O estado de reprodução mudou.
    StateChanged(PlayerState),
    /// Posição atual da reprodução.
    Position(std::time::Duration),
    /// Duração conhecida da fonte carregada.
    Duration(std::time::Duration),
    /// A fonte terminou de tocar.
    Ended,
    /// Erro de dispositivo, rede, playlist ou decodificação.
    Error(String),
}

/// Erros síncronos do contrato público e categorias internas do backend.
#[derive(Debug, Error)]
pub enum PlayerError {
    /// O volume solicitado não está no intervalo inclusivo de 0.0 a 1.0.
    #[error("volume must be within 0.0..=1.0: {0}")]
    InvalidVolume(f32),
    /// A thread dedicada não está disponível.
    #[error("audio worker is unavailable")]
    WorkerUnavailable,
    /// Não há fonte decodificada carregada.
    #[error("no audio source is loaded")]
    NoAudioLoaded,
    /// Não foi possível abrir um dispositivo de saída.
    #[error("audio output device is unavailable")]
    AudioOutputUnavailable,
    /// Falha de leitura ou escrita local.
    #[error("I/O error: {0}")]
    Io(String),
    /// Falha durante uma requisição HTTP.
    #[error("network error: {0}")]
    Network(String),
    /// Playlist HLS inválida ou fora do subconjunto implementado.
    #[error("HLS playlist error: {0}")]
    Playlist(String),
    /// Falha de demuxação ou decodificação.
    #[error("audio decoding error: {0}")]
    Decode(String),
    /// Codec ou recurso não suportado.
    #[error("unsupported audio feature: {0}")]
    Unsupported(String),
    /// URL ou protocolo de fonte inválido.
    #[error("invalid media source: {0}")]
    InvalidSource(String),
    /// Falha ao buscar a posição solicitada.
    #[error("seek failed: {0}")]
    Seek(String),
}
