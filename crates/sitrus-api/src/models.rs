//! Modelos de resposta dos endpoints da Apple Music usados pelo Sitrus: a
//! biblioteca do usuário (`/v1/me/library/*`) e o catálogo público
//! (`/v1/catalog/*`).
//!
//! Os nomes de campo espelham exatamente o JSON real devolvido pela Apple:
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

// ---------------------------------------------------------------------
// Catálogo público (`api.music.apple.com/v1/catalog`) — busca e detalhes.
// Modelos novos da fase de busca/detalhes; seguem as mesmas convenções da
// biblioteca (nomes reais do JSON, campos tolerantes) e convivem com os
// modelos `Library*` acima, que continuam intocados.
// ---------------------------------------------------------------------

/// Faixa do catálogo (`type: "songs"`): resultado de busca e também os itens
/// dos relacionamentos de faixas de álbum e playlist.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogSong {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis da faixa.
    pub attributes: CatalogSongAttributes,
}

/// Atributos visíveis de uma faixa do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogSongAttributes {
    /// Nome de exibição (`attributes.name`).
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
    /// URL da prévia de áudio de 30 s, tocável sem DRM
    /// (`attributes.previewUrl`).
    #[serde(default, rename = "previewUrl")]
    pub preview_url: Option<String>,
    /// Contagem de reproduções informada pela Apple (`attributes.playCount`).
    #[serde(default, rename = "playCount")]
    pub play_count: Option<u64>,
    /// Capa (`attributes.artwork`), com placeholders `{w}`/`{h}`.
    #[serde(default)]
    pub artwork: Option<Artwork>,
}

/// Itens de um relacionamento da resposta (`relationships.{tracks,albums}`),
/// com o cursor de continuação quando o relacionamento vem paginado.
///
/// O `bound` explícito evita os bounds imperfeitos do derive (serde issue
/// 26925), que exigiriam `T: Default` sem necessidade para o campo `data`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct RelationshipPage<T> {
    /// Itens relacionados desta página.
    #[serde(default)]
    pub data: Vec<T>,
    /// Caminho do relacionamento (`href`).
    #[serde(default)]
    pub href: Option<String>,
    /// URL absoluta da próxima página do relacionamento, quando houver
    /// (`next`).
    #[serde(default)]
    pub next: Option<String>,
}

/// Relacionamentos interpretados do álbum; só `tracks` é relevante hoje.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AlbumRelationships {
    /// Faixas do álbum, presentes com `include=tracks`.
    #[serde(default, rename = "tracks")]
    pub tracks: Option<RelationshipPage<CatalogSong>>,
}

/// Álbum do catálogo (`v1/catalog/{storefront}/albums/{id}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogAlbum {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis do álbum.
    pub attributes: CatalogAlbumAttributes,
    /// Relacionamentos devolvidos na resposta; quando o request não pediu
    /// `include`, o campo chega vazio (não é erro).
    #[serde(default)]
    pub relationships: AlbumRelationships,
}

/// Atributos visíveis de um álbum do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogAlbumAttributes {
    /// Título do álbum (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Artista principal (`attributes.artistName`).
    #[serde(default, rename = "artistName")]
    pub artist_name: Option<String>,
    /// Quantidade de faixas (`attributes.trackCount`).
    #[serde(default, rename = "trackCount")]
    pub track_count: Option<u64>,
    /// Data de lançamento conforme a Apple (ISO 8601), quando houver
    /// (`attributes.releaseDate`).
    #[serde(default, rename = "releaseDate")]
    pub release_date: Option<String>,
    /// Capa do álbum (`attributes.artwork`), com placeholders `{w}`/`{h}`.
    #[serde(default)]
    pub artwork: Option<Artwork>,
}

/// Relacionamentos interpretados do artista; só `albums` é relevante hoje.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ArtistRelationships {
    /// Álbuns do artista, presentes com `include=albums`.
    #[serde(default, rename = "albums")]
    pub albums: Option<RelationshipPage<CatalogAlbum>>,
}

/// Artista do catálogo (`v1/catalog/{storefront}/artists/{id}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogArtist {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis do artista.
    pub attributes: CatalogArtistAttributes,
    /// Relacionamentos devolvidos na resposta; ausentes sem `include`.
    #[serde(default)]
    pub relationships: ArtistRelationships,
}

/// Atributos visíveis de um artista do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogArtistAttributes {
    /// Nome de exibição (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
}

/// Relacionamentos interpretados da playlist; só `tracks` é relevante hoje.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PlaylistRelationships {
    /// Faixas da playlist, presentes com `include=tracks`.
    #[serde(default, rename = "tracks")]
    pub tracks: Option<RelationshipPage<CatalogSong>>,
}

/// Playlist de catálogo (`v1/catalog/{storefront}/playlists/{id}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogPlaylist {
    /// Identificador do item (`id`).
    pub id: String,
    /// Atributos visíveis da playlist.
    pub attributes: CatalogPlaylistAttributes,
    /// Relacionamentos devolvidos na resposta; ausentes sem `include`.
    #[serde(default)]
    pub relationships: PlaylistRelationships,
}

/// Atributos visíveis de uma playlist de catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogPlaylistAttributes {
    /// Nome de exibição (`attributes.name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Curador atribuído pela Apple ou pelo editorial
    /// (`attributes.curatorName`).
    #[serde(default, rename = "curatorName")]
    pub curator_name: Option<String>,
    /// Capa da playlist (`attributes.artwork`), com placeholders `{w}`/`{h}`.
    #[serde(default)]
    pub artwork: Option<Artwork>,
}
