//! Thread de rede do aplicativo: todo I/O remoto acontece aqui e chega à UI
//! por canal. Nenhuma requisição bloqueia o frame do egui.
//!
//! Responsabilidades:
//! - manter `TokenProvider` + `LibraryClient` (biblioteca) + `CatalogClient`
//!   (catálogo) da `sitrus-api` após o login;
//! - carregar a biblioteca do usuário (faixas e playlists) com paginação e
//!   limiar de segurança, publicando o resultado;
//! - busca multi-tipo no catálogo (`artists,albums,songs`) que dá nome às
//!   páginas navegáveis; em falha, fallback para filtro local da biblioteca;
//! - detalhes: artista (com álbuns e músicas), álbum (com faixas) e faixas
//!   de playlist (catálogo ou biblioteca, conforme a origem);
//! - baixar e decodificar capas de artwork (imagem e gradiente de hero).
//!
//! Nada aqui registra cookies, tokens ou qualquer credencial em log.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvError, Sender, channel};
use std::thread;
use std::time::Duration;

use egui::Color32;
use image::DynamicImage;
use serde::Deserialize;
use sitrus_api::catalog::CATALOG_API_BASE;
use sitrus_api::client::AMP_API_BASE;
use sitrus_api::{
    CatalogAlbum, CatalogArtist, CatalogClient, CatalogSong, LibraryClient, RelationshipPage,
    Session, TokenProvider,
};
use sitrus_core::model::{EntityId, Song};
use url::Url;

use crate::model::{AlbumRow, PlaylistRow, SearchArtistRow, SongRow, secs_from_millis};

/// Páginas máximas de faixas carregadas de uma vez (limiar de segurança;
/// 4 × 100 = 400 itens também já decidem o modo lista automático > 200 itens).
const SONG_PAGES: u32 = 4;
/// Páginas máximas de playlists carregadas de uma vez.
const PLAYLIST_PAGES: u32 = 1;
/// Tamanho de página pedido à API.
const PAGE_LIMIT: u32 = 100;
/// Limite de tamanho de um download de capa (bytes).
const ARTWORK_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Limite de resultados quando o fallback filtra a biblioteca local.
const LOCAL_SEARCH_LIMIT: usize = 50;
/// Limite por seção da busca multi-tipo (artistas/álbuns/faixas).
const SEARCH_SECTION_LIMIT: u32 = 25;

/// Origem de uma página de playlist (rota da API a usar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSource {
    /// Playlist de curadoria do catálogo (`v1/catalog/.../playlists/{id}`).
    Catalog,
    /// Playlist do usuário (`v1/me/library/playlists/{id}/tracks`).
    Library,
}

/// Comandos aceitos pelo worker de rede.
pub enum NetCommand {
    /// Instala a sessão (login manual ou webview), persiste e carrega a
    /// biblioteca. Nenhum valor de cookie/token é registrado em log.
    SetSession(Session),
    /// Descarta o cliente/sessão ativos (logout).
    ClearSession,
    /// Busca no catálogo (com fallback local em caso de falha).
    Search(String),
    /// Baixa e decodifica uma capa; `key` identifica o destino na UI.
    Artwork { key: String, url: String },
    /// Detalhe de artista do catálogo (com álbuns e músicas).
    ArtistDetail(String),
    /// Detalhe de álbum do catálogo (com faixas).
    AlbumDetail(String),
    /// Faixas de uma playlist (rota escolhida pela origem).
    PlaylistDetail { id: String, source: PlaylistSource },
}

/// Eventos publicados para a UI.
pub enum NetEvent {
    /// Sessão instalada com sucesso; biblioteca começará a carregar.
    SessionReady,
    /// Falha ao instalar a sessão (sem cookie/token válido, rede, etc.).
    LoginError(String),
    /// Aviso genérico (ex.: falha ao persistir a sessão em disco).
    Notice(String),
    /// Conjunto de faixas da biblioteca.
    LibrarySongs {
        /// Linhas acumuladas até aqui (a UI substitui por este snapshot).
        rows: Vec<SongRow>,
        /// A coleção coube por inteiro nos limiares de segurança.
        complete: bool,
    },
    /// Falha ao carregar faixas da biblioteca.
    LibrarySongsFailed(String),
    /// Conjunto de playlists da biblioteca.
    LibraryPlaylists {
        /// Linhas acumuladas.
        rows: Vec<PlaylistRow>,
        /// A coleção coube por inteiro nos limiares de segurança.
        complete: bool,
    },
    /// Falha ao carregar playlists da biblioteca.
    LibraryPlaylistsFailed(String),
    /// Resultado de busca.
    SearchDone {
        /// Termo buscado.
        term: String,
        /// Resultados (prévia do catálogo traz `preview_url`).
        rows: Vec<SongRow>,
        /// Artistas encontrados (catálogo; vazio no fallback local).
        artists: Vec<SearchArtistRow>,
        /// Álbuns encontrados (catálogo; vazio no fallback local).
        albums: Vec<AlbumRow>,
        /// Origem dos resultados, para exibição junto à lista.
        source: &'static str,
        /// Nota de fallback/limitação, quando houver.
        note: Option<String>,
    },
    /// Falha de busca (nem catálogo nem biblioteca devolveram itens).
    SearchFailed { term: String, message: String },
    /// Detalhe de artista carregado (nome + álbuns + músicas).
    ArtistDetailDone {
        /// Id do artista.
        id: String,
        /// Nome de exibição.
        name: String,
        /// Álbuns do relacionamento.
        albums: Vec<AlbumRow>,
        /// Músicas de `artists/{id}/songs`.
        songs: Vec<SongRow>,
    },
    /// Falha ao carregar o detalhe de um artista.
    ArtistDetailFailed { id: String, message: String },
    /// Detalhe de álbum carregado (com faixas).
    AlbumDetailDone {
        /// Id do álbum.
        id: String,
        /// Título do álbum.
        title: String,
        /// Subtítulo do hero (artista · ano).
        subtitle: String,
        /// Capa do álbum (template `{w}`/`{h}`).
        artwork_url: Option<String>,
        /// Faixas na ordem do álbum.
        songs: Vec<SongRow>,
    },
    /// Falha ao carregar o detalhe de um álbum.
    AlbumDetailFailed { id: String, message: String },
    /// Faixas de playlist carregadas.
    PlaylistDetailDone {
        /// Id da playlist.
        id: String,
        /// Rota usada (catálogo ou biblioteca).
        source: PlaylistSource,
        /// Nome de exibição (vazio quando a rota de biblioteca não o traz —
        /// a UI completa com o nome do card clicado).
        title: String,
        /// Subtítulo do hero.
        subtitle: String,
        /// Capa (template `{w}`/`{h}`), quando disponível.
        artwork_url: Option<String>,
        /// Faixas na ordem da playlist.
        songs: Vec<SongRow>,
    },
    /// Falha ao carregar as faixas de uma playlist.
    PlaylistDetailFailed {
        /// Id da playlist.
        id: String,
        /// Rota usada.
        source: PlaylistSource,
        /// Mensagem de erro para o banner.
        message: String,
    },
    /// Capa decodificada; `image == None` significa falha (a UI marca como
    /// tentada e não repete a requisição).
    ArtworkDone {
        key: String,
        image: Option<DynamicImage>,
        gradient: Option<(Color32, Color32)>,
    },
}

/// Estado do worker entre comandos.
struct NetState {
    http: reqwest::Client,
    tokens: Option<TokenProvider>,
    client: Option<LibraryClient>,
    /// Cliente do catálogo (`v1/catalog/*`) para busca multi-tipo e detalhes.
    catalog: Option<CatalogClient>,
    /// Último snapshot de faixas da biblioteca (filtro local da busca).
    last_songs: Vec<SongRow>,
}

/// Cria o worker e devolve os canais (`comandos → worker`, `eventos → UI`)
/// e um sender extra de eventos para produtores fora do worker: o watcher do
/// subprocesso de login por webview publica em `net_rx` a partir dele.
pub fn spawn() -> (Sender<NetCommand>, Receiver<NetEvent>, Sender<NetEvent>) {
    let (command_tx, command_rx) = channel();
    let (event_tx, event_rx) = channel();
    let worker_event_tx = event_tx.clone();
    let spawn_result = thread::Builder::new()
        .name("sitrus-net".to_owned())
        .spawn(move || run_worker(command_rx, worker_event_tx));
    if spawn_result.is_err() {
        // Sem thread de rede o app não carrega nada; sinaliza na UI.
        let _ = event_tx.send(NetEvent::LibrarySongsFailed(
            "thread de rede não pôde ser iniciada".to_owned(),
        ));
    }
    (command_tx, event_rx, event_tx)
}

/// Loop do worker: um runtime tokio local atende as futures (`LibraryClient`
/// e `TokenProvider` são async) enquanto os comandos chegam.
fn run_worker(command_rx: Receiver<NetCommand>, event_tx: Sender<NetEvent>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        let _ = event_tx.send(NetEvent::LibrarySongsFailed(
            "runtime de rede não pôde ser iniciado".to_owned(),
        ));
        return;
    };
    let client_result = reqwest::Client::builder()
        .user_agent(concat!("sitrus/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build();
    let Ok(http) = client_result else {
        let _ = event_tx.send(NetEvent::LibrarySongsFailed(
            "cliente HTTP não pôde ser inicializado".to_owned(),
        ));
        return;
    };

    let mut state = NetState {
        http,
        tokens: None,
        client: None,
        catalog: None,
        last_songs: Vec::new(),
    };

    loop {
        match command_rx.recv() {
            Ok(command) => handle_command(&runtime, &mut state, command, &event_tx),
            Err(RecvError) => return,
        }
    }
}

/// Despacha um comando para o worker.
fn handle_command(
    runtime: &tokio::runtime::Runtime,
    state: &mut NetState,
    command: NetCommand,
    event_tx: &Sender<NetEvent>,
) {
    match command {
        NetCommand::SetSession(session) => install_session(runtime, state, session, event_tx),
        NetCommand::ClearSession => {
            state.client = None;
            state.tokens = None;
            state.catalog = None;
            state.last_songs = Vec::new();
        }
        NetCommand::Search(term) => run_search(runtime, state, term, event_tx),
        NetCommand::Artwork { key, url } => run_artwork(runtime, state, key, url, event_tx),
        NetCommand::ArtistDetail(id) => run_artist_detail(runtime, state, &id, event_tx),
        NetCommand::AlbumDetail(id) => run_album_detail(runtime, state, &id, event_tx),
        NetCommand::PlaylistDetail { id, source } => {
            run_playlist_detail(runtime, state, &id, source, event_tx)
        }
    }
}

/// Persiste a sessão, constrói provider + cliente e inicia a carga da biblioteca.
fn install_session(
    runtime: &tokio::runtime::Runtime,
    state: &mut NetState,
    session: Session,
    event_tx: &Sender<NetEvent>,
) {
    if session.media_user_token.is_none() {
        let _ = event_tx.send(NetEvent::LoginError(
            "sessão sem media-user-token; entre novamente".to_owned(),
        ));
        return;
    }
    if let Err(error) = session.save() {
        let _ = event_tx.send(NetEvent::Notice(format!(
            "Sessão não foi salva em disco: {error}"
        )));
    }
    // O cliente leva o próprio provider; a busca do app usa um provider
    // dedicado (cada um descobre o bearer uma vez e o mantém em cache).
    let client_provider = TokenProvider::new(state.http.clone());
    match LibraryClient::new(client_provider, session.clone()) {
        Ok(client) => {
            state.client = Some(client);
            state.tokens = Some(TokenProvider::new(state.http.clone()));
            // Cliente do catálogo para busca multi-tipo e páginas de detalhe
            // (artista/álbum/playlist de catálogo), na mesma loja da sessão.
            let catalog_tokens = TokenProvider::new(state.http.clone());
            match CatalogClient::new(catalog_tokens, &session.storefront) {
                Ok(catalog) => state.catalog = Some(catalog),
                Err(error) => {
                    // A biblioteca continua funcionando; detalhes/catálogo
                    // falharão com mensagem própria na página.
                    let _ = event_tx.send(NetEvent::Notice(format!(
                        "Catálogo indisponível nesta sessão: {error}"
                    )));
                }
            }
            if event_tx.send(NetEvent::SessionReady).is_err() {
                return;
            }
            load_library(runtime, state, event_tx);
        }
        Err(error) => {
            let _ = event_tx.send(NetEvent::LoginError(error.to_string()));
        }
    }
}

/// Carrega faixas e playlists da biblioteca dentro dos limiares.
fn load_library(
    runtime: &tokio::runtime::Runtime,
    state: &mut NetState,
    event_tx: &Sender<NetEvent>,
) {
    let mut songs: Vec<SongRow> = Vec::new();
    let mut songs_complete = true;
    {
        let Some(client) = state.client.as_ref() else {
            return;
        };
        let mut offset = 0_u32;
        for _ in 0..SONG_PAGES {
            match runtime.block_on(client.library_songs(offset, PAGE_LIMIT)) {
                Ok(page) => {
                    let has_next = page.next.is_some();
                    let mut page_rows: Vec<SongRow> = page.data.iter().map(song_row_from).collect();
                    let play_counts = fetch_library_play_counts(runtime, state, offset, PAGE_LIMIT);
                    for row in &mut page_rows {
                        row.play_count = play_counts.get(&row.id).copied();
                    }
                    songs.extend(page_rows);
                    songs_complete = songs_complete && !has_next;
                    if !has_next {
                        break;
                    }
                    offset = offset.saturating_add(PAGE_LIMIT);
                }
                Err(error) => {
                    let _ = event_tx.send(NetEvent::LibrarySongsFailed(error.to_string()));
                    songs_complete = false;
                    break;
                }
            }
        }
    }
    state.last_songs = songs.clone();
    let _ = event_tx.send(NetEvent::LibrarySongs {
        rows: songs,
        complete: songs_complete,
    });

    let mut playlists: Vec<PlaylistRow> = Vec::new();
    let mut playlists_complete = true;
    if let Some(client) = state.client.as_ref() {
        let mut offset = 0_u32;
        for _ in 0..PLAYLIST_PAGES {
            match runtime.block_on(client.library_playlists(offset, PAGE_LIMIT)) {
                Ok(page) => {
                    let has_next = page.next.is_some();
                    playlists.extend(page.data.iter().map(playlist_row_from));
                    playlists_complete = playlists_complete && !has_next;
                    if !has_next {
                        break;
                    }
                    offset = offset.saturating_add(PAGE_LIMIT);
                }
                Err(error) => {
                    let _ = event_tx.send(NetEvent::LibraryPlaylistsFailed(error.to_string()));
                    playlists_complete = false;
                    break;
                }
            }
        }
    }
    let _ = event_tx.send(NetEvent::LibraryPlaylists {
        rows: playlists,
        complete: playlists_complete,
    });
}

#[derive(Debug, Deserialize)]
struct LibraryPlayCountPage {
    #[serde(default)]
    data: Vec<LibraryPlayCountResource>,
}

#[derive(Debug, Deserialize)]
struct LibraryPlayCountResource {
    id: String,
    #[serde(default)]
    attributes: LibraryPlayCountAttributes,
}

#[derive(Debug, Default, Deserialize)]
struct LibraryPlayCountAttributes {
    #[serde(default, rename = "playCount")]
    play_count: Option<u64>,
}

/// Lê `playCount` sem ampliar a superfície de modelos de `sitrus-api`.
/// Falha no enriquecimento não falha a biblioteca: contagens ausentes ficam
/// como `None`, enquanto a ordem original da página da API é preservada.
fn fetch_library_play_counts(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    offset: u32,
    limit: u32,
) -> HashMap<String, u64> {
    let (Some(tokens), Some(client)) = (state.tokens.as_ref(), state.client.as_ref()) else {
        return HashMap::new();
    };
    let Some(media_user_token) = client.session().media_user_token.as_deref() else {
        return HashMap::new();
    };
    let Ok(token) = runtime.block_on(tokens.token()) else {
        return HashMap::new();
    };
    let Ok(mut url) = Url::parse(AMP_API_BASE).and_then(|base| base.join("v1/me/library/songs"))
    else {
        return HashMap::new();
    };
    url.query_pairs_mut()
        .append_pair("offset", &offset.to_string())
        .append_pair("limit", &limit.to_string());
    let referer = if client.session().storefront.is_empty() {
        "https://music.apple.com/".to_owned()
    } else {
        format!("https://music.apple.com/{}/", client.session().storefront)
    };
    let page = runtime.block_on(async {
        let response = state
            .http
            .get(url)
            .bearer_auth(&token.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ORIGIN, "https://music.apple.com")
            .header(reqwest::header::REFERER, referer)
            .header("Media-User-Token", media_user_token)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<LibraryPlayCountPage>().await.ok()
    });
    page.map(|page| {
        page.data
            .into_iter()
            .filter_map(|resource| {
                resource
                    .attributes
                    .play_count
                    .map(|count| (resource.id, count))
            })
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
fn parse_library_play_counts(raw: &str) -> Result<HashMap<String, u64>, String> {
    let page: LibraryPlayCountPage = serde_json::from_str(raw)
        .map_err(|error| format!("resposta de contagens inválida: {error}"))?;
    Ok(page
        .data
        .into_iter()
        .filter_map(|resource| {
            resource
                .attributes
                .play_count
                .map(|count| (resource.id, count))
        })
        .collect())
}

/// Projeção `LibrarySong` → linha de UI + `Song` de domínio.
fn song_row_from(song: &sitrus_api::LibrarySong) -> SongRow {
    let id = song.id.clone();
    let title = song
        .attributes
        .name
        .clone()
        .unwrap_or_else(|| "Faixa sem título".to_owned());
    let artist = song.attributes.artist_name.clone().unwrap_or_default();
    let album = song.attributes.album_name.clone().unwrap_or_default();
    let subtitle = if artist.is_empty() {
        album.clone()
    } else {
        artist
    };
    let duration_millis = song.attributes.duration_in_millis.unwrap_or(0);
    let artwork_url = song
        .attributes
        .artwork
        .as_ref()
        .and_then(|artwork| artwork.url_for(300, 300));
    let domain_song = Song {
        id: EntityId::new(&id),
        title: title.clone(),
        album: EntityId::new(&album),
        duration: Some(Duration::from_millis(duration_millis)),
        preview_url: None,
    };
    SongRow {
        id,
        title,
        subtitle,
        album_name: album,
        duration_secs: secs_from_millis(duration_millis),
        artwork_url,
        preview_url: None,
        play_count: None,
        song: domain_song,
    }
}

/// Projeção de faixa de catálogo → linha de UI (mantém a prévia tocável).
fn catalog_song_row_from(song: &CatalogSong) -> SongRow {
    let id = song.id.clone();
    let attrs = &song.attributes;
    let title = attrs
        .name
        .clone()
        .unwrap_or_else(|| "Faixa sem título".to_owned());
    let artist = attrs.artist_name.clone().unwrap_or_default();
    let album = attrs.album_name.clone().unwrap_or_default();
    let subtitle = if artist.is_empty() {
        album.clone()
    } else {
        artist
    };
    let duration_millis = attrs.duration_in_millis.unwrap_or(0);
    let preview = attrs
        .preview_url
        .as_ref()
        .and_then(|raw| Url::parse(raw).ok());
    SongRow {
        song: Song {
            id: EntityId::new(&id),
            title: title.clone(),
            album: EntityId::new(&album),
            duration: (duration_millis > 0).then(|| Duration::from_millis(duration_millis)),
            preview_url: preview.clone(),
        },
        album_name: album,
        artwork_url: attrs
            .artwork
            .as_ref()
            .and_then(|artwork| artwork.url_for(300, 300)),
        duration_secs: secs_from_millis(duration_millis),
        preview_url: preview,
        play_count: attrs.play_count,
        id,
        subtitle,
        title,
    }
}

/// Projeção de artista de catálogo → linha da aba Artistas da busca.
fn artist_row_from(artist: &CatalogArtist) -> SearchArtistRow {
    SearchArtistRow {
        id: artist.id.clone(),
        name: artist
            .attributes
            .name
            .clone()
            .unwrap_or_else(|| "Artista sem nome".to_owned()),
    }
}

/// Projeção de álbum de catálogo → cartão navegável.
fn album_row_from(album: &CatalogAlbum) -> AlbumRow {
    let attrs = &album.attributes;
    AlbumRow {
        id: album.id.clone(),
        title: attrs
            .name
            .clone()
            .unwrap_or_else(|| "Álbum sem título".to_owned()),
        artist: attrs.artist_name.clone().unwrap_or_default(),
        year: attrs.release_date.as_deref().and_then(year_from_release),
        artwork_url: attrs
            .artwork
            .as_ref()
            .and_then(|artwork| artwork.url_for(300, 300)),
    }
}

/// Ano de exibição a partir da data ISO da Apple (`"2024-11-08"` → `"2024"`).
#[must_use]
fn year_from_release(release: &str) -> Option<String> {
    let year = release.get(..4)?;
    (year.len() == 4 && year.chars().all(|c| c.is_ascii_digit())).then(|| year.to_owned())
}

/// Projeção `LibraryPlaylist` → linha de UI.
///
/// `PlaylistAttributes` da `sitrus-api` expõe apenas `name`/`canEdit` — sem
/// contagem de faixas nem capa (lacuna registrada no handoff).
fn playlist_row_from(playlist: &sitrus_api::LibraryPlaylist) -> PlaylistRow {
    let name = playlist
        .attributes
        .name
        .clone()
        .unwrap_or_else(|| "Playlist sem nome".to_owned());
    PlaylistRow {
        id: playlist.id.clone(),
        name,
        track_count: None,
        artwork_url: None,
    }
}

/// Busca o catálogo com bearer do provider; em falha, filtra a biblioteca local.
fn run_search(
    runtime: &tokio::runtime::Runtime,
    state: &mut NetState,
    term: String,
    event_tx: &Sender<NetEvent>,
) {
    let term = term.trim().to_owned();
    if term.is_empty() {
        return;
    }

    let sections = match (state.tokens.as_ref(), state.client.as_ref()) {
        (Some(tokens), Some(client)) => {
            catalog_search_all(runtime, state, &term, tokens, &client.session().storefront)
        }
        _ => Err("entre para buscar no catálogo".to_owned()),
    };

    match sections {
        Ok(search) => {
            let _ = event_tx.send(NetEvent::SearchDone {
                term,
                rows: search.songs,
                artists: search.artists,
                albums: search.albums,
                source: "Catálogo da Apple Music",
                note: None,
            });
        }
        Err(message) => {
            let needle = term.to_lowercase();
            let rows: Vec<SongRow> = state
                .last_songs
                .iter()
                .filter(|row| {
                    row.title.to_lowercase().contains(&needle)
                        || row.subtitle.to_lowercase().contains(&needle)
                        || row.album_name.to_lowercase().contains(&needle)
                })
                .take(LOCAL_SEARCH_LIMIT)
                .cloned()
                .collect();
            if rows.is_empty() {
                let _ = event_tx.send(NetEvent::SearchFailed {
                    term,
                    message: format!("catálogo indisponível ({message}) e nada na sua biblioteca"),
                });
            } else {
                let _ = event_tx.send(NetEvent::SearchDone {
                    term,
                    rows,
                    artists: Vec::new(),
                    albums: Vec::new(),
                    source: "Sua biblioteca",
                    note: Some(format!("catálogo indisponível ({message})")),
                });
            }
        }
    }
}

/// Resultado da busca multi-tipo (seções independentes da resposta).
#[derive(Debug, Default)]
struct CatalogSearchSections {
    artists: Vec<SearchArtistRow>,
    albums: Vec<AlbumRow>,
    songs: Vec<SongRow>,
}

/// Envelope da resposta de busca: `results.{artists,albums,songs}`.
#[derive(Debug, Deserialize)]
struct CatalogSearchResults {
    /// Seções devolvidas pela busca (ausentes quando a seção não foi pedida
    /// ou não tem resultados).
    #[serde(default)]
    results: CatalogSearchSectionsEnvelope,
}

/// Seções da busca, cada uma com uma página de recursos.
#[derive(Debug, Default, Deserialize)]
struct CatalogSearchSectionsEnvelope {
    /// Artistas (`results.artists.data`).
    #[serde(default, rename = "artists")]
    artists: Option<RelationshipPage<CatalogArtist>>,
    /// Álbuns (`results.albums.data`).
    #[serde(default, rename = "albums")]
    albums: Option<RelationshipPage<CatalogAlbum>>,
    /// Faixas (`results.songs.data`).
    #[serde(default, rename = "songs")]
    songs: Option<RelationshipPage<CatalogSong>>,
}

/// Interpreta o envelope multi-tipo da busca em linhas de UI. Falha de
/// sintaxe/shape vira erro (o fallback local assume); seções ausentes numa
/// resposta válida viram listas vazias.
fn parse_catalog_search(raw: &str) -> Result<CatalogSearchSections, String> {
    let parsed: CatalogSearchResults = serde_json::from_str(raw.trim())
        .map_err(|error| format!("resposta de busca inválida: {error}"))?;
    Ok(CatalogSearchSections {
        artists: parsed
            .results
            .artists
            .map(|page| page.data.iter().map(artist_row_from).collect())
            .unwrap_or_default(),
        albums: parsed
            .results
            .albums
            .map(|page| page.data.iter().map(album_row_from).collect())
            .unwrap_or_default(),
        songs: parsed
            .results
            .songs
            .map(|page| page.data.iter().map(catalog_song_row_from).collect())
            .unwrap_or_default(),
    })
}

/// Sobe `v1/catalog/{storefront}/search?term&types&limit` com bearer e devolve
/// (status, corpo).
async fn send_catalog_request(
    http: &reqwest::Client,
    url: Url,
    bearer: &str,
) -> Result<(u16, String), String> {
    let response = http
        .get(url)
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|error| error.to_string())?;
    Ok((status, body))
}

/// Requisição `v1/catalog/{storefront}/search` com bearer para as três
/// seções (artists, albums, songs) usadas pela tela Busca.
///
/// O envio é multi-tipo e o parsing das seções é local (`parse_catalog_search`):
/// a `CatalogClient::search` de `sitrus-api` ainda devolve só a seção `songs`.
fn catalog_search_all(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    term: &str,
    tokens: &TokenProvider,
    storefront: &str,
) -> Result<CatalogSearchSections, String> {
    let token = runtime
        .block_on(tokens.token())
        .map_err(|error| error.to_string())?;
    let storefront = sanitize_storefront(storefront);
    let mut url = Url::parse(CATALOG_API_BASE)
        .and_then(|base| base.join(&format!("v1/catalog/{storefront}/search")))
        .map_err(|error| format!("URL de busca inválida: {error}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("term", term);
        pairs.append_pair("types", "artists,albums,songs");
        pairs.append_pair("limit", &SEARCH_SECTION_LIMIT.to_string());
    }

    let (status, body) = runtime.block_on(send_catalog_request(&state.http, url, &token.token))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    parse_catalog_search(&body)
}

/// Loja alvo saneada (ascii alfanumérico minúsculo; padrão `us`).
fn sanitize_storefront(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        "us".to_owned()
    } else {
        cleaned
    }
}

/// Detalhe de artista do catálogo: `artist_detail` (com álbuns) + `artist_songs`.
/// Qualquer falha nas duas etapas manda a página inteira para `Failed`.
fn run_artist_detail(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    id: &str,
    event_tx: &Sender<NetEvent>,
) {
    let Some(catalog) = state.catalog.as_ref() else {
        let _ = event_tx.send(NetEvent::ArtistDetailFailed {
            id: id.to_owned(),
            message: "sessão sem cliente do catálogo".to_owned(),
        });
        return;
    };
    let artist = match runtime.block_on(catalog.artist_detail(id)) {
        Ok(artist) => artist,
        Err(error) => {
            let _ = event_tx.send(NetEvent::ArtistDetailFailed {
                id: id.to_owned(),
                message: error.to_string(),
            });
            return;
        }
    };
    let albums = artist
        .relationships
        .albums
        .as_ref()
        .map(|page| page.data.iter().map(album_row_from).collect())
        .unwrap_or_default();
    let songs = match runtime.block_on(catalog.artist_songs(id)) {
        Ok(page) => page.data.iter().map(catalog_song_row_from).collect(),
        Err(error) => {
            let _ = event_tx.send(NetEvent::ArtistDetailFailed {
                id: id.to_owned(),
                message: format!("faixas do artista: {error}"),
            });
            return;
        }
    };
    let name = artist
        .attributes
        .name
        .clone()
        .unwrap_or_else(|| "Artista sem nome".to_owned());
    let _ = event_tx.send(NetEvent::ArtistDetailDone {
        id: id.to_owned(),
        name,
        albums,
        songs,
    });
}

/// Detalhe de álbum do catálogo: `album_detail` com `include=tracks`.
fn run_album_detail(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    id: &str,
    event_tx: &Sender<NetEvent>,
) {
    let Some(catalog) = state.catalog.as_ref() else {
        let _ = event_tx.send(NetEvent::AlbumDetailFailed {
            id: id.to_owned(),
            message: "sessão sem cliente do catálogo".to_owned(),
        });
        return;
    };
    let album = match runtime.block_on(catalog.album_detail(id)) {
        Ok(album) => album,
        Err(error) => {
            let _ = event_tx.send(NetEvent::AlbumDetailFailed {
                id: id.to_owned(),
                message: error.to_string(),
            });
            return;
        }
    };
    let title = album
        .attributes
        .name
        .clone()
        .unwrap_or_else(|| "Álbum sem título".to_owned());
    let subtitle = album_subtitle(
        &album.attributes.artist_name,
        &album.attributes.release_date,
    );
    let artwork_url = album
        .attributes
        .artwork
        .as_ref()
        .and_then(|artwork| artwork.url_for(600, 600));
    let songs = album
        .relationships
        .tracks
        .as_ref()
        .map(|page| page.data.iter().map(catalog_song_row_from).collect())
        .unwrap_or_default();
    let _ = event_tx.send(NetEvent::AlbumDetailDone {
        id: id.to_owned(),
        title,
        subtitle,
        artwork_url,
        songs,
    });
}

/// Subtítulo do hero de álbum/playlist: artista · ano (quando extraível).
#[must_use]
fn album_subtitle(artist: &Option<String>, release_date: &Option<String>) -> String {
    let artist = artist.clone().unwrap_or_default();
    let year = release_date.as_deref().and_then(year_from_release);
    match (artist.is_empty(), year) {
        (true, Some(year)) => year,
        (true, None) => String::new(),
        (false, Some(year)) => format!("{artist} · {year}"),
        (false, None) => artist,
    }
}

/// Faixas de playlist: catálogo (curadoria, com `include=tracks`) ou
/// biblioteca do usuário (`/library/playlists/{id}/tracks`).
fn run_playlist_detail(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    id: &str,
    source: PlaylistSource,
    event_tx: &Sender<NetEvent>,
) {
    match source {
        PlaylistSource::Catalog => {
            let Some(catalog) = state.catalog.as_ref() else {
                let _ = event_tx.send(NetEvent::PlaylistDetailFailed {
                    id: id.to_owned(),
                    source,
                    message: "sessão sem cliente do catálogo".to_owned(),
                });
                return;
            };
            let playlist = match runtime.block_on(catalog.playlist_detail(id)) {
                Ok(playlist) => playlist,
                Err(error) => {
                    let _ = event_tx.send(NetEvent::PlaylistDetailFailed {
                        id: id.to_owned(),
                        source,
                        message: error.to_string(),
                    });
                    return;
                }
            };
            let title = playlist
                .attributes
                .name
                .clone()
                .unwrap_or_else(|| "Playlist sem nome".to_owned());
            let curator = playlist
                .attributes
                .curator_name
                .clone()
                .unwrap_or_else(|| "Catálogo da Apple Music".to_owned());
            let subtitle = format!("Curadoria · {curator}");
            let artwork_url = playlist
                .attributes
                .artwork
                .as_ref()
                .and_then(|artwork| artwork.url_for(600, 600));
            let songs = playlist
                .relationships
                .tracks
                .as_ref()
                .map(|page| page.data.iter().map(catalog_song_row_from).collect())
                .unwrap_or_default();
            let _ = event_tx.send(NetEvent::PlaylistDetailDone {
                id: id.to_owned(),
                source,
                title,
                subtitle,
                artwork_url,
                songs,
            });
        }
        PlaylistSource::Library => {
            let Some(client) = state.client.as_ref() else {
                let _ = event_tx.send(NetEvent::PlaylistDetailFailed {
                    id: id.to_owned(),
                    source,
                    message: "sessão ausente; entre novamente".to_owned(),
                });
                return;
            };
            // O endpoint de faixas não devolve atributos da playlist: o nome
            // é completado pela UI a partir do card clicado.
            let page = match runtime.block_on(client.library_playlist_tracks(id, 0, PAGE_LIMIT)) {
                Ok(page) => page,
                Err(error) => {
                    let _ = event_tx.send(NetEvent::PlaylistDetailFailed {
                        id: id.to_owned(),
                        source,
                        message: error.to_string(),
                    });
                    return;
                }
            };
            let songs = page.data.iter().map(song_row_from).collect();
            let _ = event_tx.send(NetEvent::PlaylistDetailDone {
                id: id.to_owned(),
                source,
                title: String::new(),
                subtitle: "Playlist da sua biblioteca".to_owned(),
                artwork_url: None,
                songs,
            });
        }
    }
}

/// Baixa bytes de um URL com limite de tamanho.
async fn fetch_artwork_bytes(http: &reqwest::Client, url: Url) -> Option<Vec<u8>> {
    let response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    if response
        .content_length()
        .is_some_and(|len| len > ARTWORK_MAX_BYTES)
    {
        return None;
    }
    let data = response.bytes().await.ok()?;
    let data = data.to_vec();
    if (data.len() as u64) > ARTWORK_MAX_BYTES {
        None
    } else {
        Some(data)
    }
}

/// Baixa, decodifica e deriva o gradiente de uma capa.
fn run_artwork(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    key: String,
    url: String,
    event_tx: &Sender<NetEvent>,
) {
    let fetched = match Url::parse(&url) {
        Ok(url) => runtime.block_on(fetch_artwork_bytes(&state.http, url)),
        Err(_) => None,
    };
    let (image, gradient) = match fetched {
        Some(bytes) => match sitrus_ui::artwork::decode_artwork(&bytes) {
            Some(image) => {
                let gradient = sitrus_ui::artwork::hero_gradient(&image);
                (Some(image), Some(gradient))
            }
            None => (None, None),
        },
        None => (None, None),
    };
    // Se o worker já não existe, a UI igualmente marcará a capa como tentada.
    let _ = event_tx.send(NetEvent::ArtworkDone {
        key,
        image,
        gradient,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Envelope multi-tipo completo: artistas, álbuns e faixas com os campos
    /// que a UI consome (nome/cartão navegável, prévia tocável, duração).
    #[test]
    fn parse_search_reads_all_three_sections() {
        let raw = r#"{
            "results": {
                "artists": {"href": "/artists", "data": [
                    {"id": "7260", "type": "artists", "attributes": {"name": "Sitrus Studio"}}
                ]},
                "albums": {"data": [
                    {"id": "42", "type": "albums", "attributes": {
                        "name": "Midnight Citrus", "artistName": "Sitrus Studio",
                        "trackCount": 10, "releaseDate": "2026-03-01",
                        "artwork": {"url": "https://img/{w}x{h}bb.jpg"}
                    }}
                ]},
                "songs": {"data": [
                    {"id": "900", "type": "songs", "attributes": {
                        "name": "Citrus Dawn", "artistName": "Sitrus Studio",
                        "albumName": "Midnight Citrus", "durationInMillis": 212000,
                        "previewUrl": "https://audio.mzstatic.com/p.mp3",
                        "playCount": 5,
                        "artwork": {"url": "https://img/{w}x{h}bb.jpg"}
                    }}
                ]}
            }
        }"#;
        let sections = parse_catalog_search(raw).expect("envelope válido");
        assert!(
            sections.artists.len() == 1,
            "artistas: {:?}",
            sections.artists
        );
        let artist = &sections.artists[0];
        assert_eq!(artist.id, "7260");
        assert_eq!(artist.name, "Sitrus Studio");

        assert_eq!(sections.albums.len(), 1);
        let album = &sections.albums[0];
        assert_eq!(album.id, "42");
        assert_eq!(album.title, "Midnight Citrus");
        assert_eq!(album.artist, "Sitrus Studio");
        assert_eq!(album.year.as_deref(), Some("2026"));
        assert_eq!(
            album.artwork_url.as_deref(),
            Some("https://img/300x300bb.jpg")
        );

        assert_eq!(sections.songs.len(), 1);
        let song = &sections.songs[0];
        assert_eq!(song.id, "900");
        assert_eq!(song.title, "Citrus Dawn");
        assert_eq!(song.subtitle, "Sitrus Studio");
        assert_eq!(song.duration_secs, 212);
        assert_eq!(song.play_count, Some(5));
        assert!(song.preview_url.is_some());
        assert_eq!(
            song.preview_url.as_ref().unwrap().as_str(),
            "https://audio.mzstatic.com/p.mp3"
        );
        // Faixa de domínio pronta para a fila com a prévia preservada.
        assert!(song.song.preview_url.is_some());
    }

    /// Resposta válida sem seções (sem resultados) devolve listas vazias.
    #[test]
    fn parse_search_with_empty_results_yields_empty_sections() {
        let raw = r#"{"results": {}}"#;
        let sections = parse_catalog_search(raw).expect("envelope válido");
        assert!(sections.artists.is_empty());
        assert!(sections.albums.is_empty());
        assert!(sections.songs.is_empty());
    }

    /// JSON quebrado vira erro (o fallback local assume a busca).
    #[test]
    fn parse_search_rejects_malformed_body() {
        assert!(parse_catalog_search("{\"results\": [").is_err());
        assert!(parse_catalog_search("").is_err());
    }

    /// Falta de `previewUrl` não gera URL; durações de 0 ficam sem duracão.
    #[test]
    fn catalog_song_without_preview_maps_cleanly() {
        let raw = r#"{"results": {"songs": {"data": [
            {"id": "1", "attributes": {"name": "S", "durationInMillis": 0}}
        ]}}}"#;
        let sections = parse_catalog_search(raw).expect("envelope válido");
        let song = &sections.songs[0];
        assert!(song.preview_url.is_none());
        assert_eq!(song.duration_secs, 0);
        assert_eq!(song.subtitle, ""); // sem artista nem álbum
        assert_eq!(song.title, "S");
    }

    /// Ano extraído de datas ISO curtas; formatos fora do padrão viram None.
    #[test]
    fn year_from_release_extracts_head_digits() {
        assert_eq!(year_from_release("2024-11-08").as_deref(), Some("2024"));
        assert_eq!(year_from_release("2024").as_deref(), Some("2024"));
        assert_eq!(year_from_release("ab-12"), None);
        assert_eq!(year_from_release("20"), None);
        assert_eq!(year_from_release(""), None);
    }

    #[test]
    fn album_subtitle_composes_artist_and_year() {
        assert_eq!(
            album_subtitle(&Some("Artista".to_owned()), &Some("2026-01-01".to_owned())),
            "Artista · 2026"
        );
        assert_eq!(
            album_subtitle(&Some("Artista".to_owned()), &None),
            "Artista"
        );
        assert_eq!(album_subtitle(&None, &Some("1999".to_owned())), "1999");
        assert_eq!(album_subtitle(&None, &None), "");
    }

    #[test]
    fn parse_library_play_counts_ignores_missing_counts_and_keeps_zero() {
        let raw = r#"{"data":[
            {"id":"a","attributes":{"playCount":7}},
            {"id":"b","attributes":{"playCount":0}},
            {"id":"c","attributes":{}}
        ]}"#;
        let counts = parse_library_play_counts(raw).expect("contagens válidas");
        assert_eq!(counts.get("a"), Some(&7));
        assert_eq!(counts.get("b"), Some(&0));
        assert!(!counts.contains_key("c"));
    }

    #[test]
    fn parse_library_play_counts_rejects_malformed_json() {
        assert!(parse_library_play_counts("not-json").is_err());
    }
}
