//! Sitrus-api: cliente do player web da Apple Music (biblioteca do usuário).
//!
//! Estrutura da crate:
//!
//! - [`token`]: descoberta e cache do bearer (JWT) de `music.apple.com`;
//! - [`session`]: media-user-token, cookies e storefront, com persistência em
//!   `~/.local/share/sitrus/session.json` (0600) e entrada manual por cookies
//!   colados (`Session::from_pasted_cookies`), sem webview;
//! - [`client`]: endpoints `/v1/me/library/*` em `amp-api.music.apple.com`
//!   (**não oficiais** — veja o aviso no topo do módulo) com retry/backoff;
//! - [`models`]: tipos serde com os nomes reais dos campos do JSON da Apple;
//! - [`error`]: erros tipados (`ApiError`) e o mapeamento status → erro;
//! - [`search`]: busca de catálogo legada herdada do scaffold (stub de envio).

pub mod client;
pub mod error;
pub mod models;
pub mod search;
pub mod session;
pub mod token;

pub use client::{LibraryClient, RetryPolicy};
pub use error::{ApiError, ApiResult};
pub use models::{Artwork, LibraryAlbum, LibraryArtist, LibraryPlaylist, LibrarySong, Page};
pub use search::{API_BASE, ApiCredentials, AppleMusicClient, parse_search_response};
pub use session::Session;
pub use token::{DeveloperToken, TokenProvider};
