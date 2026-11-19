//! Modelos de resposta dos endpoints de biblioteca (`/v1/me/library/*`).
//!
//! Os nomes de campo espelham exatamente o JSON real devolvido pelo player web:
//! `attributes.name`, `attributes.artistName`, `attributes.albumName`,
//! `attributes.durationInMillis` e `attributes.artwork.url`. Campos
//! desconhecidos são descartados pelo serde (sem `deny_unknown_fields`), então
//! atributos novos da Apple não quebram o parser; campos que podem faltar são
//! `Option` com `#[serde(default)]`.

use serde::Deserialize;

/// Página genérica de listagem, com cursor de continuação.
///
/// O derive não usa `#[serde(default)]` no campo genérico — isso empurraria um
/// bound espúrio `T: Default` (serde issue 26925). Um envelope sem `data` é
/// tolerado pelo [`crate::client::parse_page`], que interpreta via
/// `serde_json::Value` e devolve página vazia.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Page<T> {
    /// Itens desta página.
    pub data: Vec<T>,
    /// URL absoluta da próxima página, quando ainda há itens. Ausente no
    /// JSON devolve `None`.
    pub next: Option<String>,
}

/// Capa do item; a URL da Apple é um template contendo `{w}` e `{h}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artwork {
    /// URL de template, ex.: `.../{w}x{h}bb.jpg`.
    #[serde(default)]
    pub url: Option<String>,
    /// Largura original sugerida, em pixels.
    #[serde(default)]
    pub width: Option<u64>,
    /// Altura original sugerida, em pixels.
    #[serde(default)]
    pub height: Option<u64>,
}

impl Artwork {
    /// URL pronta no tamanho pedido, com os placeholders `{w}` e `{h}`
    /// substituídos. Templates sem placeholders são devolvidos como estão.
    #[must_use]
    pub fn url_for(&self, width: u32, height: u32) -> Option<String> {
        let template = self.url.as_ref()?;
        let sized = template
            .replace("{w}", &width.to_string())
            .replace("{h}", &height.to_string());
        Some(sized)
    }
}

/// Faixa da biblioteca do usuário (`/v1/me/library/songs`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LibrarySong {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis da faixa.
    pub attributes: SongAttributes,
}

/// Atributos visíveis de uma faixa da biblioteca.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SongAttributes {
    /// Nome de exibição da faixa (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Artista principal (`attributes.artistName`).
    #[serde(default, rename = "artistName")]
    pub artist_name: Option<String>,
    /// Álbum ao qual a faixa pertence (`attributes.albumName`).
    #[serde(default, rename = "albumName")]
    pub album_name: Option<String>,
    /// Duração declarada, em milissegundos (`attributes.durationInMillis`).
    #[serde(default, rename = "durationInMillis")]
    pub duration_in_millis: Option<u64>,
    /// Capa do álbum (`attributes.artwork`), com placeholders `{w}`/`{h}`.
    #[serde(default)]
    pub artwork: Option<Artwork>,
}

/// Artista da biblioteca do usuário (`/v1/me/library/artists`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LibraryArtist {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis do artista.
    pub attributes: ArtistAttributes,
}

/// Atributos visíveis de um artista da biblioteca.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArtistAttributes {
    /// Nome de exibição do artista (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
}

/// Álbum da biblioteca do usuário (`/v1/me/library/albums`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LibraryAlbum {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis do álbum.
    pub attributes: AlbumAttributes,
}

/// Atributos visíveis de um álbum da biblioteca.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AlbumAttributes {
    /// Título do álbum (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Artista principal (`attributes.artistName`).
    #[serde(default, rename = "artistName")]
    pub artist_name: Option<String>,
    /// Quantidade de faixas (`attributes.trackCount`).
    #[serde(default, rename = "trackCount")]
    pub track_count: Option<u64>,
    /// Data de lançamento conforme a Apple (ISO 8601), quando houver.
    #[serde(default, rename = "releaseDate")]
    pub release_date: Option<String>,
    /// Capa do álbum (`attributes.artwork`), com placeholders `{w}`/`{h}`.
    #[serde(default)]
    pub artwork: Option<Artwork>,
}

/// Playlist da biblioteca do usuário (`/v1/me/library/playlists`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LibraryPlaylist {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis da playlist.
    pub attributes: PlaylistAttributes,
}

/// Atributos visíveis de uma playlist da biblioteca.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlaylistAttributes {
    /// Nome de exibição da playlist (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Verdadeiro quando o usuário pode editar a playlist
    /// (`attributes.canEdit`).
    #[serde(default, rename = "canEdit")]
    pub can_edit: Option<bool>,
}
