//! Sitrus-api: cliente dos endpoints da Apple Music usados pelo Sitrus, com
//! tokens descobertos do player web (sem token de desenvolvedor próprio).
//!
//! Estrutura da crate:
//!
//! - [`token`]: descoberta e cache do bearer (JWT) de `music.apple.com`;
//! - [`session`]: media-user-token, cookies e storefront, com persistência em
//!   `~/.local/share/sitrus/session.json` (0600) e entrada manual por cookies
//!   colados (`Session::from_pasted_cookies`), sem webview;
//! - [`client`]: endpoints `/v1/me/library/*` em `amp-api.music.apple.com`
//!   (**não oficiais** — veja o aviso no topo do módulo) com retry/backoff;
//! - [`catalog`]: catálogo público em `api.music.apple.com` (`search` com
//!   `term`/`types`/`limit`, detalhes de álbum/artista/playlist e faixas de
//!   artista) com o bearer do [`token`] — **uso não oficial** da Apple Music
//!   API, veja o aviso no módulo;
//! - [`models`]: tipos serde com os nomes reais dos campos do JSON da Apple;
//! - [`error`]: erros tipados (`ApiError`) e o mapeamento status → erro;
//! - [`search`]: busca de catálogo legada herdada do scaffold: mantém o
//!   parser `parse_search_response` (faixas → `Track` do domínio) usado pelo
//!   fallback de busca do app; o envio real passou para [`catalog`].
//!
//! Os testes não dependem de rede: o parsing é coberto por fixtures em
//! `tests/fixtures/` e o envio, por um servidor HTTP de loopback interno
//! (`src/test_support.rs`, só para testes).

pub mod catalog;
pub mod client;
pub mod error;
pub mod models;
pub mod search;
pub mod session;
pub mod token;

#[cfg(test)]
pub(crate) mod test_support;

pub use catalog::{
    CATALOG_API_BASE, CatalogClient, catalog_url, parse_catalog_resource, parse_search_page,
    search_url,
};
pub use client::{LibraryClient, RetryPolicy};
pub use error::{ApiError, ApiResult};
pub use models::{
    Artwork, CatalogAlbum, CatalogArtist, CatalogPlaylist, CatalogSong, LibraryAlbum,
    LibraryArtist, LibraryPlaylist, LibrarySong, Page, RelationshipPage,
};
pub use search::{API_BASE, ApiCredentials, AppleMusicClient, parse_search_response};
pub use session::Session;
pub use token::{DeveloperToken, TokenProvider};
