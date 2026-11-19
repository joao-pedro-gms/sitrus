//! Engine de reprodução DRM do Sitrus.
//!
//! Define a trait [`Engine`] — o contrato comum que os backends de reprodução
//! implementam — mais os tipos próprios [`EngineState`]/[`EngineEvent`] e
//! [`QueueItem`], sem depender de `sitrus-app`.
//!
//! Duas implementações acompanham o crate:
//!
//! - [`MockEngine`]: simuladora determinística em Rust para testes de
//!   contrato, sem Node, sem Electron e sem dispositivo de áudio.
//! - [`SidecarEngine`]: cliente do sidecar `engine/` (Electron castlabs com
//!   CDM Widevine), que fala JSON-RPC 2.0 sobre WebSocket restrito a
//!   `127.0.0.1`, porta efêmera e token de sessão gerado por execução. Quando
//!   o sidecar não sobe, o erro é tipado ([`EngineError::requires_native_fallback`])
//!   para que o app caia no `sitrus-player` nativo.

mod engine;
mod error;
mod mock;
pub mod rpc;
mod sidecar;

pub use engine::{Engine, EngineEvent, EngineState, QueueItem};
pub use error::{EngineError, EngineErrorKind};
pub use mock::MockEngine;
pub use rpc::{
    CookieObservation, SessionProbe, SidecarConfig, SidecarCookie, SidecarMode, SidecarPage,
    SidecarSession,
};
pub use sidecar::SidecarEngine;
