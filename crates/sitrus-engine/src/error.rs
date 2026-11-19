//! Erros tipados do engine e a classificação usada pelo fallback nativo.

use thiserror::Error;

use crate::rpc;

/// Erros síncronos do contrato [`crate::Engine`] e do cliente de sidecar.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum EngineError {
    /// O volume solicitado não está no intervalo inclusivo de 0.0 a 1.0.
    #[error("volume must be within 0.0..=1.0: {0}")]
    VolumeOutOfRange(f32),
    /// Item de fila inválido (por exemplo, sem identificador).
    #[error("queue item is invalid: {reason}")]
    InvalidQueueItem {
        /// Motivo da rejeição.
        reason: String,
    },
    /// A sessão Apple transportada ao sidecar é inválida (envelope vazio,
    /// cookie sem nome/valor, limites excedidos). Entrada local do consumidor.
    #[error("sidecar session is invalid: {reason}")]
    SessionInvalid {
        /// Motivo da rejeição.
        reason: String,
    },
    /// O sidecar não está disponível (Node ausente, `engine/` incompleto,
    /// processo morreu antes do handshake). Sinal de fallback nativo.
    #[error("sidecar is unavailable: {reason}")]
    SidecarUnavailable {
        /// Motivo técnico (sem segredos).
        reason: String,
    },
    /// O sidecar subiu mas não anunciou a linha de pronto a tempo.
    #[error("sidecar did not announce readiness within {timeout_ms} ms")]
    SidecarTimedOut {
        /// Prazo configurado, em milissegundos.
        timeout_ms: u64,
    },
    /// O token de sessão foi recusado ou a conexão inicial caiu.
    #[error("sidecar handshake was rejected: {reason}")]
    HandshakeRejected {
        /// Motivo informado pelo sidecar.
        reason: String,
    },
    /// O WebSocket para o sidecar foi perdido em pleno funcionamento.
    #[error("sidecar connection was lost")]
    ConnectionLost,
    /// O sidecar não respondeu o comando dentro do prazo configurado.
    #[error("sidecar rpc timed out after {timeout_ms} ms")]
    RpcTimedOut {
        /// Prazo configurado, em milissegundos.
        timeout_ms: u64,
    },
    /// O sidecar respondeu com um erro JSON-RPC (fase de comando/reprodução).
    #[error("sidecar rpc error {code}: {message}")]
    Rpc {
        /// Código JSON-RPC devolvido pelo sidecar.
        code: i32,
        /// Mensagem do sidecar (sem segredos).
        message: String,
    },
    /// Mensagem fora do protocolo vinda do sidecar.
    #[error("invalid message from sidecar: {reason}")]
    InvalidMessage {
        /// Motivo da rejeição.
        reason: String,
    },
}

/// Categoria do erro, para decisões de política no consumidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineErrorKind {
    /// Sidecar indiponível em qualquer fase (spawn, pronto, conexão).
    SidecarUnavailable,
    /// Autenticação/handshake recusados.
    Handshake,
    /// Falha reportada pelo protocolo JSON-RPC do sidecar.
    Rpc,
    /// Entrada local inválida (volume, item de fila).
    InvalidInput,
    /// Mensagem fora do protocolo.
    Protocol,
}

impl EngineError {
    /// Classifica o erro conforme [`EngineErrorKind`].
    #[must_use]
    pub fn kind(&self) -> EngineErrorKind {
        match self {
            Self::VolumeOutOfRange(_)
            | Self::InvalidQueueItem { .. }
            | Self::SessionInvalid { .. } => EngineErrorKind::InvalidInput,
            Self::SidecarUnavailable { .. }
            | Self::SidecarTimedOut { .. }
            | Self::ConnectionLost => EngineErrorKind::SidecarUnavailable,
            Self::HandshakeRejected { .. } => EngineErrorKind::Handshake,
            Self::Rpc { .. } => EngineErrorKind::Rpc,
            Self::RpcTimedOut { .. } => EngineErrorKind::Rpc,
            Self::InvalidMessage { .. } => EngineErrorKind::Protocol,
        }
    }

    /// Indica que o consumidor deve cair no `sitrus-player` nativo: o sidecar
    /// não está disponível, o handshake falhou ou a conexão/protocolo quebrou.
    /// Falhas de reprodução reportadas pelo protocolo (`EngineErrorKind::Rpc`)
    /// **não** pedem fallback.
    #[must_use]
    pub fn requires_native_fallback(&self) -> bool {
        matches!(
            self.kind(),
            EngineErrorKind::SidecarUnavailable
                | EngineErrorKind::Handshake
                | EngineErrorKind::Protocol
        )
    }

    /// Mapeia a resposta de erro JSON-RPC do sidecar para o erro tipado; o
    /// código de token recusado vira [`EngineError::HandshakeRejected`].
    #[must_use]
    pub fn from_rpc_error(code: i32, message: &str) -> Self {
        if code == rpc::RPC_INVALID_TOKEN {
            Self::HandshakeRejected {
                reason: message.to_owned(),
            }
        } else {
            Self::Rpc {
                code,
                message: message.to_owned(),
            }
        }
    }
}
