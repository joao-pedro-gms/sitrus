//! Cliente do catálogo público da Apple Music (`api.music.apple.com`).
//!
//! # Aviso: uso não oficial dos endpoints
//!
//! O Sitrus não é um desenvolvedor registrado na Apple Music API: não há
//! token de desenvolvedor próprio, e o bearer usado aqui é o do player web,
//! obtido por [`crate::token`] — a mesma via dos endpoints de biblioteca em
//! [`crate::client`]. A Apple não oferece garantia de estabilidade para esse
//! uso: pode alterar caminhos, campos do JSON, regras da API ou rejeitar o
//! token capturado a qualquer momento, sem aviso — o que quebrará o Sitrus.
//! Para limitar o estrago, todo o parsing está isolado em [`crate::models`]
//! (campos desconhecidos são descartados) e coberto por fixtures offline em
//! `tests/fixtures/`.
//!
//! Cada requisição envia o bearer do token em cache (renovado sob demanda),
//! repete 429/5xx com backoff exponencial ([`crate::client::RetryPolicy`]) e
//! mapeia 401 para [`ApiError::Unauthorized`]. A paginação segue o cursor
//! `next`, sempre validado contra o host da API antes do envio.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::client::{
    PLAYER_ORIGIN, RetryPolicy, execute_with_retry, parse_page, parse_retry_after, sanitize_next,
    validate_id,
};
use crate::error::{ApiError, ApiResult};
use crate::models::{CatalogAlbum, CatalogArtist, CatalogPlaylist, CatalogSong, Page};
use crate::token::{DeveloperToken, TokenProvider};

/// URL base da API pública da Apple Music (a barra final importa para
/// `Url::join`).
pub const CATALOG_API_BASE: &str = "https://api.music.apple.com/";

/// Cliente dos endpoints de catálogo (`v1/catalog/*`) em
/// `api.music.apple.com`, autenticado com o bearer do player web. **Uso não
/// oficial** — veja o aviso no topo do módulo.
#[derive(Debug, Clone)]
pub struct CatalogClient {
    http: reqwest::Client,
    base: Url,
    storefront: String,
    tokens: TokenProvider,
    retry: RetryPolicy,
}

impl CatalogClient {
    /// Constrói o cliente a partir do provedor de bearer e da loja alvo
    /// (storefront, ex.: `"br"` — normalizada para minúsculas).
    ///
    /// # Erros
    /// [`ApiError::Network`] quando o TLS/reqwest não puder inicializar;
    /// [`ApiError::Parse`] nunca deve ocorrer (URL base é literal).
    pub fn new(tokens: TokenProvider, storefront: &str) -> ApiResult<Self> {
        let base = Url::parse(CATALOG_API_BASE)
            .map_err(|e| ApiError::Parse(format!("URL base do catálogo inválida: {e}")))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("sitrus/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base,
            storefront: storefront.trim().to_ascii_lowercase(),
            tokens,
            retry: RetryPolicy::default(),
        })
    }

    /// Substitui a política padrão de retry (encadeável).
    #[must_use]
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Loja em uso nos caminhos `v1/catalog/{storefront}/...`.
    #[must_use]
    pub fn storefront(&self) -> &str {
        &self.storefront
    }

    /// Base alternativa para os testes de envio (servidor de loopback).
    /// Recria o cliente HTTP sem proxies de ambiente, para que os testes não
    /// dependam de variáveis `*_PROXY` do host.
    #[cfg(test)]
    pub(crate) fn with_base(mut self, base: Url) -> ApiResult<Self> {
        self.base = base;
        self.http = reqwest::Client::builder()
            .user_agent(concat!("sitrus/", env!("CARGO_PKG_VERSION")))
            .no_proxy()
            .build()?;
        Ok(self)
    }

    /// Busca no catálogo: GET em `v1/catalog/{storefront}/search` com `term`,
    /// `types` (recursos separados por vírgula, ex.: `"songs"`) e `limit` na
    /// query. O parser devolve a página da seção `songs` — hoje o Sitrus só
    /// consome faixas da busca.
    ///
    /// # Erros
    /// [`ApiError::Unauthorized`] em 401, [`ApiError::RateLimited`] em 429
    /// (após esgotar o retry), [`ApiError::Http`] para os demais status e
    /// [`ApiError::Parse`] quando o JSON não casa com os modelos.
    pub async fn search(
        &self,
        term: &str,
        types: &str,
        limit: u32,
    ) -> ApiResult<Page<CatalogSong>> {
        let url = search_url(&self.base, &self.storefront, term, types, limit)?;
        self.execute_page(url, parse_search_page).await
    }

    /// Detalhe de um álbum com `include=tracks`: as faixas vêm em
    /// `relationships.tracks`.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando o `id` é vazio ou contém `/`; demais erros
    /// herdados de [`Self::execute_page`].
    pub async fn album_detail(&self, id: &str) -> ApiResult<CatalogAlbum> {
        validate_id(id, "do álbum")?;
        let mut url = catalog_url(&self.base, &self.storefront, &format!("albums/{id}"))?;
        url.query_pairs_mut().append_pair("include", "tracks");
        self.execute_page(url, parse_catalog_resource::<CatalogAlbum>)
            .await
    }

    /// Detalhe de um artista com `include=albums`: os álbuns vêm em
    /// `relationships.albums`.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando o `id` é vazio ou contém `/`; demais erros
    /// herdados de [`Self::execute_page`].
    pub async fn artist_detail(&self, id: &str) -> ApiResult<CatalogArtist> {
        validate_id(id, "do artista")?;
        let mut url = catalog_url(&self.base, &self.storefront, &format!("artists/{id}"))?;
        url.query_pairs_mut().append_pair("include", "albums");
        self.execute_page(url, parse_catalog_resource::<CatalogArtist>)
            .await
    }

    /// Faixas de catálogo do artista, em
    /// `v1/catalog/{storefront}/artists/{id}/songs`.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando o `id` é vazio ou contém `/`; demais erros
    /// herdados de [`Self::execute_page`].
    pub async fn artist_songs(&self, id: &str) -> ApiResult<Page<CatalogSong>> {
        validate_id(id, "do artista")?;
        let url = catalog_url(&self.base, &self.storefront, &format!("artists/{id}/songs"))?;
        self.execute_page(url, parse_page::<CatalogSong>).await
    }

    /// Detalhe de uma playlist (curadoria) com `include=tracks`: as faixas
    /// vêm em `relationships.tracks`.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando o `id` é vazio ou contém `/`; demais erros
    /// herdados de [`Self::execute_page`].
    pub async fn playlist_detail(&self, id: &str) -> ApiResult<CatalogPlaylist> {
        validate_id(id, "da playlist")?;
        let mut url = catalog_url(&self.base, &self.storefront, &format!("playlists/{id}"))?;
        url.query_pairs_mut().append_pair("include", "tracks");
        self.execute_page(url, parse_catalog_resource::<CatalogPlaylist>)
            .await
    }

    /// Segue o cursor `next` devolvido por uma página anterior do catálogo.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando a URL `next` não é absoluta ou aponta para
    /// um host diferente da API do catálogo.
    pub async fn fetch_next<T>(&self, next: &str) -> ApiResult<Page<T>>
    where
        T: DeserializeOwned,
    {
        let url = sanitize_next(&self.base, next)?;
        self.execute_page(url, parse_page::<T>).await
    }

    /// Envia a requisição (com retry) e classifica a resposta com o parser
    /// `parse`; status diferente de 200 segue o mapeamento de [`ApiError`].
    async fn execute_page<T>(&self, url: Url, parse: fn(&str) -> ApiResult<T>) -> ApiResult<T> {
        let (status, retry_after, body) = self.dispatch(url).await?;
        match status {
            200 => parse(&body),
            other => Err(ApiError::from_status(other, &body, retry_after)),
        }
    }

    /// Monta a requisição autenticada: bearer do token em cache + origem e
    /// referer do player web (mesmíssimo disfarce do [`crate::client`]).
    fn request(&self, url: Url, token: &DeveloperToken) -> reqwest::RequestBuilder {
        let referer = format!("{PLAYER_ORIGIN}/{}/", self.storefront);
        self.http
            .get(url)
            .bearer_auth(&token.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ORIGIN, PLAYER_ORIGIN)
            .header(reqwest::header::REFERER, referer)
    }

    /// Garante o bearer (cache ou descoberta do player) e executa a
    /// requisição com retry/backoff devolvendo `(status, retry_after, corpo)`.
    async fn dispatch(&self, url: Url) -> ApiResult<(u16, Option<Duration>, String)> {
        let token = self.tokens.token().await?;
        // `RequestBuilder` não é `Clone` no reqwest 0.13: cada tentativa
        // reconstrói a requisição a partir da URL e do token.
        execute_with_retry(&self.retry, || {
            let token = token.clone();
            let url = url.clone();
            async move { self.probe(self.request(url, &token)).await }
        })
        .await
    }

    /// Executa uma requisição e devolve `(status, retry_after, corpo)`.
    async fn probe(
        &self,
        request: reqwest::RequestBuilder,
    ) -> ApiResult<(u16, Option<Duration>, String)> {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let body = response.text().await?;
        Ok((status, retry_after, body))
    }
}

/// Monta a URL absoluta de um recurso do catálogo:
/// `v1/catalog/{storefront}/{subpath}` contra a `base`. A storefront deve já
/// estar normalizada (o cliente normaliza na construção).
///
/// # Erros
/// [`ApiError::Parse`] se o subcaminho não resolver contra a base (proteção
/// contra subcaminhos malformados).
pub fn catalog_url(base: &Url, storefront: &str, subpath: &str) -> ApiResult<Url> {
    let path = format!("v1/catalog/{storefront}/{subpath}");
    base.join(&path)
        .map_err(|e| ApiError::Parse(format!("endpoint de catálogo inválido: {e}")))
}

/// URL da busca com `term`, `types` (recursos separados por vírgula) e
/// `limit` na query, nesta ordem.
///
/// # Erros
/// [`ApiError::Parse`] herdado de [`catalog_url`].
pub fn search_url(
    base: &Url,
    storefront: &str,
    term: &str,
    types: &str,
    limit: u32,
) -> ApiResult<Url> {
    let mut url = catalog_url(base, storefront, "search")?;
    url.query_pairs_mut()
        .append_pair("term", term)
        .append_pair("types", types)
        .append_pair("limit", &limit.to_string());
    Ok(url)
}

/// Interpreta a página da seção `songs` de uma resposta de busca
/// (`{"results": {"songs": {"data": [...], "next": ...}}}`). Envelope sem a
/// seção devolve página vazia — busca sem resultados não é erro.
///
/// # Erros
/// [`ApiError::Parse`] quando o JSON está truncado ou uma faixa não casa com
/// [`CatalogSong`].
pub fn parse_search_page(raw: &str) -> ApiResult<Page<CatalogSong>> {
    let envelope: SearchEnvelope = serde_json::from_str(raw).map_err(|e| {
        ApiError::Parse(format!(
            "JSON de busca do catálogo não pôde ser interpretado: {e}"
        ))
    })?;
    let Some(songs) = envelope.results.and_then(|results| results.songs) else {
        return Ok(Page {
            data: Vec::new(),
            next: None,
        });
    };
    Ok(Page {
        data: songs.data,
        next: songs.next,
    })
}

/// Interpreta a resposta de um recurso único do catálogo (`{"data": [item]}`)
/// devolvendo o primeiro item — detalhes de álbum, artista e playlist.
///
/// # Erros
/// [`ApiError::Parse`] quando o JSON está truncado, `data` não tem itens ou o
/// item não casa com o modelo pedido.
pub fn parse_catalog_resource<T: DeserializeOwned>(raw: &str) -> ApiResult<T> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        ApiError::Parse(format!(
            "JSON de detalhe do catálogo não pôde ser interpretado: {e}"
        ))
    })?;
    let first = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| ApiError::Parse("resposta de detalhe chegou sem itens".to_owned()))?;
    serde_json::from_value(first.clone())
        .map_err(|e| ApiError::Parse(format!("item de detalhe não pôde ser interpretado: {e}")))
}

/// Envelope da busca do catálogo; demais seções (`albums`, `playlists`, ...)
/// são descartadas pelo parser.
#[derive(Debug, Deserialize)]
struct SearchEnvelope {
    /// Seção `results` da busca; ausente devolve página vazia.
    #[serde(default)]
    results: Option<SearchResults>,
}

/// Recursos devolvidos pela busca; hoje só `songs` é interpretado.
#[derive(Debug, Deserialize)]
struct SearchResults {
    /// Feed de faixas da busca.
    #[serde(default)]
    songs: Option<SearchSongsFeed>,
}

/// Feed de faixas dentro da resposta de busca.
#[derive(Debug, Deserialize)]
struct SearchSongsFeed {
    /// Faixas da página.
    #[serde(default)]
    data: Vec<CatalogSong>,
    /// Cursor absoluto da próxima página, quando há mais resultados.
    #[serde(default)]
    next: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockServer, seeded_provider};

    /// Lê um fixture de `tests/fixtures/`.
    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("fixture {name} não pôde ser lido: {e}"))
    }

    /// Pares `chave=valor` da query do destino registrado pelo mock.
    fn query_params(target: &str) -> Vec<(String, String)> {
        let raw = target.split_once('?').map(|(_, raw)| raw).unwrap_or("");
        raw.split('&')
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) => (key.to_owned(), value.to_owned()),
                None => (pair.to_owned(), String::new()),
            })
            .collect()
    }

    /// Valor da `key` na query, se presente.
    fn query_value<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
        params
            .iter()
            .find(|entry| entry.0 == key)
            .map(|entry| entry.1.as_str())
    }

    /// Storefront é normalizada (trim + minúsculas) antes de compor caminhos.
    #[tokio::test(flavor = "current_thread")]
    async fn storefront_is_normalized() {
        let client = CatalogClient::new(seeded_provider("tok").await, " BR ").expect("cliente");
        assert_eq!(client.storefront(), "br");
    }

    /// A busca faz GET no caminho certo, com bearer do provider e query
    /// `term`/`types`/`limit`, e interpreta o fixture real (ids, previewUrl,
    /// playCount, artwork e cursor `next`).
    #[tokio::test(flavor = "current_thread")]
    async fn search_sends_bearer_and_query_and_parses_fixture() {
        let server = MockServer::start(vec![(200, fixture("catalog_search.json"))]);
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada");

        let page = client
            .search("citrus", "songs", 25)
            .await
            .expect("busca no catálogo");

        let request = server.request(0);
        assert!(
            request.target.starts_with("/v1/catalog/br/search?"),
            "destino inesperado: {}",
            request.target
        );
        let params = query_params(&request.target);
        assert_eq!(query_value(&params, "term"), Some("citrus"));
        assert_eq!(query_value(&params, "types"), Some("songs"));
        assert_eq!(query_value(&params, "limit"), Some("25"));
        assert_eq!(
            request.header("authorization"),
            Some("Bearer test-bearer-cat")
        );
        assert_eq!(request.header("origin"), Some("https://music.apple.com"));

        assert_eq!(page.data[0].id, "song-101");
        let attributes = &page.data[0].attributes;
        assert_eq!(attributes.name.as_deref(), Some("Ponte de Sol"));
        assert_eq!(
            attributes.preview_url.as_deref(),
            Some("https://audio-ssl.itunes.apple.com/preview/song-101.m4a")
        );
        assert_eq!(attributes.play_count, Some(1234));
        assert!(attributes.artwork.is_some());
        // Faixa sem campos opcionais chega como `None`, sem quebrar a página.
        assert_eq!(page.data[1].attributes.play_count, None);
        assert!(page.next.is_some(), "cursor `next` preservado");
    }

    /// Álbum, artista, playlist e faixas do artista batem nos caminhos e
    /// `include` esperados e interpretam os fixtures de detalhe.
    #[tokio::test(flavor = "current_thread")]
    async fn detail_endpoints_hit_expected_paths_and_parse() {
        let server = MockServer::start(vec![
            (200, fixture("catalog_album.json")),
            (200, fixture("catalog_artist.json")),
            (200, fixture("catalog_playlist.json")),
            (200, fixture("catalog_artist_songs.json")),
        ]);
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada");

        let album = client.album_detail("alb-1").await.expect("álbum");
        let artist = client.artist_detail("art-1").await.expect("artista");
        let playlist = client.playlist_detail("pl-1").await.expect("playlist");
        let songs = client
            .artist_songs("art-1")
            .await
            .expect("faixas do artista");

        assert!(
            server
                .request(0)
                .target
                .starts_with("/v1/catalog/br/albums/alb-1?"),
            "destino do álbum: {}",
            server.request(0).target
        );
        assert_eq!(
            query_value(&query_params(&server.request(0).target), "include"),
            Some("tracks")
        );
        assert!(
            server
                .request(1)
                .target
                .starts_with("/v1/catalog/br/artists/art-1?")
        );
        assert_eq!(
            query_value(&query_params(&server.request(1).target), "include"),
            Some("albums")
        );
        assert!(
            server
                .request(2)
                .target
                .starts_with("/v1/catalog/br/playlists/pl-1?")
        );
        assert_eq!(
            query_value(&query_params(&server.request(2).target), "include"),
            Some("tracks")
        );
        assert_eq!(
            server.request(3).target,
            "/v1/catalog/br/artists/art-1/songs",
            "faixas do artista não levam query extra"
        );

        // Parsing: álbum com trackCount e faixas no relacionamento.
        assert_eq!(album.id, "alb-1");
        assert_eq!(album.attributes.track_count, Some(12));
        let tracks = album
            .relationships
            .tracks
            .as_ref()
            .expect("faixas incluídas");
        assert_eq!(tracks.data.len(), 2);
        assert_eq!(
            tracks.data[0].attributes.preview_url.as_deref(),
            Some("https://audio-ssl.itunes.apple.com/preview/song-101.m4a")
        );
        // Artista com álbuns no relacionamento.
        assert_eq!(artist.attributes.name.as_deref(), Some("Orquestra Sitrus"));
        let albums = artist
            .relationships
            .albums
            .as_ref()
            .expect("álbuns incluídos");
        assert_eq!(albums.data.len(), 2);
        assert_eq!(
            albums.data[0].attributes.name.as_deref(),
            Some("Jardim Interno")
        );
        // Playlist com capa e faixas.
        assert_eq!(playlist.attributes.name.as_deref(), Some("Foco"));
        assert_eq!(
            playlist.attributes.curator_name.as_deref(),
            Some("Sitrus Editorial")
        );
        assert!(playlist.attributes.artwork.is_some());
        assert_eq!(playlist.relationships.tracks.expect("trk").data.len(), 2);
        // Coleção de faixas do artista com cursor.
        assert_eq!(songs.data.len(), 2);
        assert_eq!(songs.data[1].attributes.play_count, Some(77));
        assert!(songs.next.is_some());
    }

    /// 401 do catálogo vira `Unauthorized` com o corpo anexado — uma única
    /// chamada (401 nunca é retriado).
    #[tokio::test(flavor = "current_thread")]
    async fn search_401_maps_to_unauthorized() {
        let server = MockServer::start(vec![(401, fixture("unauthorized.json"))]);
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada");

        let error = client
            .search("citrus", "songs", 25)
            .await
            .expect_err("401 vira erro");

        let ApiError::Unauthorized(message) = error else {
            panic!("401 deve virar ApiError::Unauthorized");
        };
        assert!(
            message.contains("Music User Token"),
            "corpo anexado: {message}"
        );
        assert_eq!(server.request_count(), 1, "401 não gera nova tentativa");
    }

    /// 429 é repetido com backoff curto e a segunda tentativa devolve a
    /// página inteira.
    #[tokio::test(flavor = "current_thread")]
    async fn rate_limited_response_is_retried() {
        let fast = RetryPolicy {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(4),
        };
        let server = MockServer::start(vec![
            (429, String::from("{}")),
            (200, fixture("catalog_search.json")),
        ]);
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada")
            .with_retry_policy(fast);

        let page = client
            .search("citrus", "songs", 25)
            .await
            .expect("segunda tentativa é bem-sucedida");

        assert_eq!(server.request_count(), 2);
        assert_eq!(server.request(0).target, server.request(1).target);
        assert_eq!(page.data[0].id, "song-101");
    }

    /// `fetch_next` segue um cursor absoluto do próprio host (aqui, loopback)
    /// e interpreta a segunda página.
    #[tokio::test(flavor = "current_thread")]
    async fn fetch_next_follows_cursor_to_second_page() {
        let server = MockServer::start_with(|base| {
            // A primeira página carrega um cursor apontando para o próprio
            // mock; a segunda resposta é o fixture de coleção do artista.
            let next_url =
                format!("{base}v1/catalog/br/search?term=citrus&types=songs&limit=2&offset=2");
            let first = format!(
                r#"{{"results":{{"songs":{{"data":[{{"id":"song-201","attributes":{{"name":"Primeira"}}}}],"next":"{next_url}"}}}}}}"#
            );
            vec![(200, first), (200, fixture("catalog_artist_songs.json"))]
        });
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada");

        let page1 = client
            .search("citrus", "songs", 2)
            .await
            .expect("primeira página");
        assert_eq!(page1.data[0].id, "song-201");
        let next = page1.next.as_deref().expect("cursor da primeira página");

        let page2 = client
            .fetch_next::<CatalogSong>(next)
            .await
            .expect("segunda página");

        assert_eq!(
            server.request(1).target,
            "/v1/catalog/br/search?term=citrus&types=songs&limit=2&offset=2"
        );
        assert_eq!(page2.data[0].id, "song-101");
        assert!(
            page2
                .next
                .as_deref()
                .is_some_and(|next| next.contains("/v1/catalog/br/artists/art-1/songs")),
            "cursor da segunda página: {:?}",
            page2.next
        );
    }

    /// 404 vira `Http(404)` com o corpo — status fora do mapa de erros.
    #[tokio::test(flavor = "current_thread")]
    async fn not_found_maps_to_http_variant() {
        let server = MockServer::start(vec![(404, String::from("not found"))]);
        let client = CatalogClient::new(seeded_provider("test-bearer-cat").await, "br")
            .expect("cliente de catálogo")
            .with_base(server.base.clone())
            .expect("base de loopback aplicada");

        let error = client
            .playlist_detail("pl-404")
            .await
            .expect_err("404 vira erro");

        assert!(matches!(error, ApiError::Http(404, body) if body.contains("not found")));
    }
}
