//! Thread de rede do aplicativo: todo I/O remoto acontece aqui e chega à UI
//! por canal. Nenhuma requisição bloqueia o frame do egui.
//!
//! Responsabilidades:
//! - manter `TokenProvider` + `LibraryClient` da `sitrus-api` após o login;
//! - carregar a biblioteca do usuário (faixas e playlists) com paginação e
//!   limiar de segurança, publicando o resultado;
//! - buscar no catálogo (enquanto o envio permanece stub em `sitrus-api`, o
//!   app monta a requisição a partir de `API_BASE` + bearer do
//!   `TokenProvider` e interpreta com `parse_search_response`), com
//!   fallback para filtro local da biblioteca;
//! - baixar e decodificar capas de artwork (imagem e gradiente de hero).
//!
//! Nada aqui registra cookies, tokens ou qualquer credencial em log.

use std::sync::mpsc::{Receiver, RecvError, Sender, channel};
use std::thread;
use std::time::Duration;

use egui::Color32;
use image::DynamicImage;
use sitrus_api::search::{API_BASE, parse_search_response};
use sitrus_api::{LibraryClient, Session, TokenProvider};
use sitrus_core::model::{EntityId, Song};
use url::Url;

use crate::model::{PlaylistRow, SongRow, secs_from_millis};

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
        /// Origem dos resultados, para exibição junto à lista.
        source: &'static str,
        /// Nota de fallback/limitação, quando houver.
        note: Option<String>,
    },
    /// Falha de busca (nem catálogo nem biblioteca devolveram itens).
    SearchFailed { term: String, message: String },
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
    /// Último snapshot de faixas da biblioteca (filtro local da busca).
    last_songs: Vec<SongRow>,
}

/// Cria o worker e devolve os canais (`comandos → worker`, `eventos → UI`).
pub fn spawn() -> (Sender<NetCommand>, Receiver<NetEvent>) {
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
    (command_tx, event_rx)
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
            state.last_songs = Vec::new();
        }
        NetCommand::Search(term) => run_search(runtime, state, term, event_tx),
        NetCommand::Artwork { key, url } => run_artwork(runtime, state, key, url, event_tx),
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
    match LibraryClient::new(client_provider, session) {
        Ok(client) => {
            state.client = Some(client);
            state.tokens = Some(TokenProvider::new(state.http.clone()));
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
                    songs.extend(page.data.iter().map(song_row_from));
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
        song: domain_song,
    }
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

    let catalog = match (state.tokens.as_ref(), state.client.as_ref()) {
        (Some(tokens), Some(client)) => {
            catalog_search(runtime, state, &term, tokens, &client.session().storefront)
        }
        _ => Err("entre para buscar no catálogo".to_owned()),
    };

    match catalog {
        Ok(rows) => {
            let _ = event_tx.send(NetEvent::SearchDone {
                term,
                rows,
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
                    source: "Sua biblioteca",
                    note: Some(format!("catálogo indisponível ({message})")),
                });
            }
        }
    }
}

/// Sobe `v1/catalog/{storefront}/search?types=songs` com bearer e devolve
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

/// Requisição `v1/catalog/{storefront}/search` com bearer.
///
/// Enquanto `AppleMusicClient::search_tracks` permanece stub de envio em
/// `sitrus-api`, o app monta a requisição com `API_BASE` + `TokenProvider` e
/// reutiliza o parser público `parse_search_response` — lacuna registrada no
/// handoff para ser movida para dentro de `sitrus-api`.
fn catalog_search(
    runtime: &tokio::runtime::Runtime,
    state: &NetState,
    term: &str,
    tokens: &TokenProvider,
    storefront: &str,
) -> Result<Vec<SongRow>, String> {
    let token = runtime
        .block_on(tokens.token())
        .map_err(|error| error.to_string())?;
    let storefront = sanitize_storefront(storefront);
    let mut url = Url::parse(API_BASE)
        .and_then(|base| base.join(&format!("v1/catalog/{storefront}/search")))
        .map_err(|error| format!("URL de busca inválida: {error}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("term", term);
        pairs.append_pair("types", "songs");
        pairs.append_pair("limit", "25");
    }

    let (status, body) = runtime.block_on(send_catalog_request(&state.http, url, &token.token))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let tracks = parse_search_response(&body).map_err(|error| error.to_string())?;
    Ok(tracks.iter().map(catalog_row_from).collect())
}

/// Projeção de `Track` de catálogo → linha de UI (mantém a prévia).
fn catalog_row_from(track: &Song) -> SongRow {
    let album = track.album.as_str();
    SongRow {
        id: track.id.as_str().to_owned(),
        title: track.title.clone(),
        subtitle: if album.is_empty() {
            "Catálogo".to_owned()
        } else {
            album.to_owned()
        },
        album_name: album.to_owned(),
        duration_secs: track.duration.as_ref().map_or(0_u32, |duration| {
            secs_from_millis(duration.as_millis() as u64)
        }),
        artwork_url: None,
        preview_url: track.preview_url.clone(),
        song: track.clone(),
    }
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
