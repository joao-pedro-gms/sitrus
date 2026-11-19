//! Modelos de domínio do catálogo e da biblioteca local.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

/// Identificador opaco de uma entidade do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    /// Cria um identificador a partir do valor bruto.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        Self(raw.to_owned())
    }

    /// Retorna o identificador como texto.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Artista creditado no catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    /// Identificador opaco do artista.
    pub id: EntityId,
    /// Nome de exibição.
    pub name: String,
}

/// Álbum associado a um artista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    /// Identificador opaco do álbum.
    pub id: EntityId,
    /// Título do álbum.
    pub title: String,
    /// Identificador do artista principal.
    pub artist: EntityId,
}

/// Faixa reproduzível, independente de backend de áudio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// Identificador opaco da faixa.
    pub id: EntityId,
    /// Título de exibição.
    pub title: String,
    /// Identificador do álbum associado.
    pub album: EntityId,
    /// Duração conhecida da faixa.
    pub duration: Option<Duration>,
    /// URL opcional de prévia.
    pub preview_url: Option<Url>,
}

impl Song {
    /// Cria uma faixa mínima sem duração nem prévia.
    #[must_use]
    pub fn new(id: EntityId, title: &str, album: EntityId) -> Self {
        Self {
            id,
            title: title.to_owned(),
            album,
            duration: None,
            preview_url: None,
        }
    }

    /// Retorna o título pronto para exibição.
    #[must_use]
    pub fn display_title(&self) -> &str {
        &self.title
    }
}

/// Alias legado mantido para os consumidores existentes do workspace.
pub type Track = Song;

/// Playlist de faixas identificadas no catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    /// Identificador opaco da playlist.
    pub id: EntityId,
    /// Nome de exibição.
    pub title: String,
    /// Faixas, na ordem da playlist.
    pub songs: Vec<EntityId>,
}

/// Artwork remoto com uma URL que pode conter placeholders de dimensão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artwork {
    /// URL base do artwork, possivelmente com `{w}` e `{h}`.
    pub url: Url,
}

impl Artwork {
    /// Retorna uma URL quadrada para o tamanho solicitado.
    #[must_use]
    pub fn url_for_size(&self, size: u32) -> Url {
        self.url_for_dimensions(size, size)
    }

    /// Expande placeholders de largura e altura, quando presentes.
    #[must_use]
    pub fn url_for_dimensions(&self, width: u32, height: u32) -> Url {
        let expanded = self
            .url
            .as_str()
            .replace("{w}", &width.to_string())
            .replace("{h}", &height.to_string())
            .replace("%7Bw%7D", &width.to_string())
            .replace("%7Bh%7D", &height.to_string());
        Url::parse(&expanded).unwrap_or_else(|_| self.url.clone())
    }
}

/// Seção da biblioteca local exibida pela aplicação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySection {
    /// Faixas salvas na biblioteca.
    Songs,
    /// Álbuns salvos na biblioteca.
    Albums,
    /// Artistas salvos na biblioteca.
    Artists,
    /// Playlists salvas na biblioteca.
    Playlists,
}

impl LibrarySection {
    /// Nome estável da seção para configuração e persistência.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Songs => "songs",
            Self::Albums => "albums",
            Self::Artists => "artists",
            Self::Playlists => "playlists",
        }
    }
}
