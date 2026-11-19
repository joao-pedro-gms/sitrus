//! Sitrus-core: modelo de domínio e utilidades essenciais do Sitrus.
//!
//! Não conhece UI nem rede; as demais crates dependem dele para compartilhar
//! os tipos da Apple Music, configurações e caminhos do usuário.

pub mod config;
pub mod error;
pub mod model;
pub mod paths;
pub mod player_state;
pub mod queue;
pub mod util;

pub use config::{Settings, Theme};
pub use error::CoreError;
pub use model::{Album, Artist, Artwork, EntityId, LibrarySection, Playlist, Song, Track};
pub use player_state::{PlaybackStatus, PlayerState};
pub use queue::PlaybackQueue;
