//! Estado do carregamento das páginas de detalhe (artista, álbum, playlist).
//!
//! Cada página é identificada por kind + id; o `DetailStore` guarda a fase
//! (`Loading`/`Loaded`/`Failed') e decide quando um comando de rede novo é
//! necessário (primeira abertura ou re-tentativa após falha — nunca enquanto
//! carrega nem para páginas na cache). Tudo puro e testável sem egui.

use std::collections::HashMap;

use crate::model::{AlbumRow, SongRow};
use crate::net::{NetCommand, PlaylistSource};

/// Origem/rota de carregamento da página de detalhe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    /// Artista do catálogo (busca).
    Artist,
    /// Álbum do catálogo (busca ou página de artista).
    Album,
    /// Playlist de curadoria do catálogo (busca).
    PlaylistCatalog,
    /// Playlist da biblioteca do usuário (tela Playlists/Home).
    PlaylistLibrary,
}

impl DetailKind {
    /// Prefixo da chave na loja de detalhes (fonte de playlist não separa
    /// catálogo/biblioteca: a origem vive no estado).
    #[must_use]
    pub const fn key_prefix(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::PlaylistCatalog | Self::PlaylistLibrary => "playlist",
        }
    }

    /// Chave da página (`prefixo:id`).
    #[must_use]
    pub fn key(self, id: &str) -> String {
        format!("{}:{id}", self.key_prefix())
    }

    /// Traduz a origem de playlist para o kind correspondente.
    #[must_use]
    pub const fn from_playlist_source(source: PlaylistSource) -> Self {
        match source {
            PlaylistSource::Catalog => Self::PlaylistCatalog,
            PlaylistSource::Library => Self::PlaylistLibrary,
        }
    }

    /// Comando de rede que carrega esta página.
    #[must_use]
    pub fn command(self, id: &str) -> NetCommand {
        match self {
            Self::Artist => NetCommand::ArtistDetail(id.to_owned()),
            Self::Album => NetCommand::AlbumDetail(id.to_owned()),
            Self::PlaylistCatalog => NetCommand::PlaylistDetail {
                id: id.to_owned(),
                source: PlaylistSource::Catalog,
            },
            Self::PlaylistLibrary => NetCommand::PlaylistDetail {
                id: id.to_owned(),
                source: PlaylistSource::Library,
            },
        }
    }
}

/// Dados prontos de uma página de detalhe (o que o hero e o corpo exibem).
#[derive(Debug, Clone)]
pub enum DetailData {
    /// Artista: nome, álbuns e músicas do catálogo.
    Artist {
        /// Nome de exibição.
        name: String,
        /// Álbuns do relacionamento (relações com obras).
        albums: Vec<AlbumRow>,
        /// Faixas de `artists/{id}/songs`.
        songs: Vec<SongRow>,
        /// Aba corrente (0 = Álbuns, 1 = Músicas).
        tab: usize,
    },
    /// Álbum: título, subtítulo composto e faixas do relacionamento.
    Album {
        /// Título do álbum.
        title: String,
        /// Subtítulo do hero (artista e ano).
        subtitle: String,
        /// Capa do álbum (com placeholders `{w}`/`{h}`).
        artwork_url: Option<String>,
        /// Faixas na ordem do álbum.
        songs: Vec<SongRow>,
    },
    /// Playlist (catálogo ou biblioteca): título, subtítulo e faixas.
    Playlist {
        /// Nome de exibição.
        title: String,
        /// Subtítulo do hero (curadoria ou origem).
        subtitle: String,
        /// Capa da playlist, quando a resposta traz.
        artwork_url: Option<String>,
        /// Faixas na ordem da playlist.
        songs: Vec<SongRow>,
    },
}

/// Fase de carregamento da página.
#[derive(Debug, Clone, Default)]
pub enum DetailPhase {
    /// Aguardando o worker de rede.
    #[default]
    Loading,
    /// Dados prontos para renderizar (cache válida enquanto durar a sessão).
    Loaded(DetailData),
    /// Falha com mensagem pronta para o botão "Tentar de novo".
    Failed(String),
}

impl DetailPhase {
    /// `true` durante o ciclo de carregamento (repaint contínuo).
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// Entrada de uma página na loja: fase + kind que a originou (o kind permite
/// re-tentar fontes de playlist corretamente).
#[derive(Debug, Clone)]
pub struct DetailState {
    /// Fase corrente.
    pub phase: DetailPhase,
    /// Kind que pediu o carregamento.
    pub kind: DetailKind,
}

/// Loja de páginas de detalhe por chave `prefixo:id`.
#[derive(Debug, Default)]
pub struct DetailStore {
    entries: HashMap<String, DetailState>,
}

impl DetailStore {
    /// Decide se a página precisa de um novo comando de rede: `Some(command)`
    /// na primeira abertura e após falha; `None` em cache válida ou quando
    /// já está em carga. No pedido, marca a página como `Loading`.
    pub fn request(&mut self, kind: DetailKind, id: &str) -> Option<NetCommand> {
        let key = kind.key(id);
        if let Some(state) = self.entries.get(&key)
            && !matches!(state.phase, DetailPhase::Failed(_))
        {
            return None;
        }
        self.entries.insert(
            key,
            DetailState {
                phase: DetailPhase::Loading,
                kind,
            },
        );
        Some(kind.command(id))
    }

    /// Fase corrente da página, se existir.
    #[must_use]
    pub fn phase(&self, key: &str) -> Option<&DetailPhase> {
        self.entries.get(key).map(|state| &state.phase)
    }

    /// Kind que originou a página (para re-tentar com a origem certa).
    #[must_use]
    pub fn kind_at(&self, key: &str) -> Option<DetailKind> {
        self.entries.get(key).map(|state| state.kind)
    }

    /// Alguma página em carregamento (repaint contínuo de skeletons).
    #[must_use]
    pub fn any_loading(&self) -> bool {
        self.entries.values().any(|state| state.phase.is_loading())
    }

    /// Instala dados prontos (evento de rede).
    pub fn apply_loaded(&mut self, kind: DetailKind, id: &str, data: DetailData) {
        let key = kind.key(id);
        self.entries.insert(
            key,
            DetailState {
                phase: DetailPhase::Loaded(data),
                kind,
            },
        );
    }

    /// Instala falha (evento de rede); preserva o kind original quando a
    /// página já existia.
    pub fn apply_failed(&mut self, kind: DetailKind, id: &str, message: String) {
        let key = kind.key(id);
        let stored = self
            .entries
            .get(&key)
            .map(|state| state.kind)
            .unwrap_or(kind);
        self.entries.insert(
            key,
            DetailState {
                phase: DetailPhase::Failed(message),
                kind: stored,
            },
        );
    }

    /// Troca a aba corrente da página de artista (ignora páginas ausentes ou
    /// de outro tipo).
    pub fn set_artist_tab(&mut self, id: &str, tab: usize) {
        let key = DetailKind::Artist.key(id);
        let Some(state) = self.entries.get_mut(&key) else {
            return;
        };
        let DetailPhase::Loaded(DetailData::Artist { tab: current, .. }) = &mut state.phase else {
            return;
        };
        *current = tab;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NetCommand as Command;

    fn song(id: &str) -> SongRow {
        SongRow {
            id: id.to_owned(),
            title: format!("Faixa {id}"),
            subtitle: String::new(),
            album_name: String::new(),
            duration_secs: 100,
            artwork_url: None,
            preview_url: None,
            play_count: None,
            song: sitrus_core::model::Song::new(
                sitrus_core::model::EntityId::new(id),
                "Faixa",
                sitrus_core::model::EntityId::new("a"),
            ),
        }
    }

    /// Primeiro pedido emit comando; segundo (em carga) não repete.
    #[test]
    fn request_dedupes_while_loading() {
        let mut store = DetailStore::default();
        let first = match store.request(DetailKind::Album, "9") {
            Some(Command::AlbumDetail(id)) => id,
            Some(_) => panic!("comando do kind errado na primeira abertura"),
            None => panic!("primeira abertura deve pedir carga"),
        };
        assert_eq!(first, "9");
        assert!(
            store.request(DetailKind::Album, "9").is_none(),
            "em carga não se repetem comandos"
        );
        assert!(store.phase("album:9").is_some_and(DetailPhase::is_loading));
    }

    /// Falha habilita re-tentativa; dados prontos voltam a bloquear.
    #[test]
    fn failed_page_retries_then_caches() {
        let mut store = DetailStore::default();
        assert!(store.request(DetailKind::Artist, "5").is_some());
        store.apply_failed(DetailKind::Artist, "5", "HTTP 401".to_owned());
        assert!(matches!(
            store.phase("artist:5"),
            Some(DetailPhase::Failed(_))
        ));
        // Re-tentativa devolve comando e volta a Loading.
        assert!(matches!(
            store.request(DetailKind::Artist, "5"),
            Some(Command::ArtistDetail(_))
        ));
        store.apply_loaded(
            DetailKind::Artist,
            "5",
            DetailData::Artist {
                name: "Artista".to_owned(),
                albums: Vec::new(),
                songs: vec![song("1")],
                tab: 0,
            },
        );
        assert!(matches!(
            store.phase("artist:5"),
            Some(DetailPhase::Loaded(DetailData::Artist { tab: 0, .. }))
        ));
        // Cache quente: sem novo comando.
        assert!(
            store.request(DetailKind::Artist, "5").is_none(),
            "página em cache não re-emite comando"
        );
    }

    /// Playlists de biblioteca e catálogo compartilham a chave, mas a
    /// re-tentativa usa o kind gravado na entrada.
    #[test]
    fn playlist_kind_is_preserved_for_retry() {
        let mut store = DetailStore::default();
        assert!(store.request(DetailKind::PlaylistLibrary, "p.7").is_some());
        // Evento marca falha; kind pedido (Library) é preservado.
        store.apply_failed(DetailKind::PlaylistCatalog, "p.7", "offline".to_owned());
        assert_eq!(
            store.kind_at("playlist:p.7"),
            Some(DetailKind::PlaylistLibrary)
        );
        let retry = store.request(DetailKind::PlaylistLibrary, "p.7");
        assert!(matches!(
            retry,
            Some(Command::PlaylistDetail {
                source: PlaylistSource::Library,
                ..
            })
        ));
    }

    /// Troca de aba só afeta páginas de artista carregadas.
    #[test]
    fn artist_tab_updates() {
        let mut store = DetailStore::default();
        assert!(store.request(DetailKind::Artist, "3").is_some());
        store.apply_loaded(
            DetailKind::Artist,
            "3",
            DetailData::Artist {
                name: "N".to_owned(),
                albums: Vec::new(),
                songs: Vec::new(),
                tab: 0,
            },
        );
        store.set_artist_tab("3", 1);
        assert!(matches!(
            store.phase("artist:3"),
            Some(DetailPhase::Loaded(DetailData::Artist { tab: 1, .. }))
        ));
    }
}
