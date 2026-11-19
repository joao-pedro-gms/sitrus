//! Cliente dos endpoints de biblioteca do player web da Apple Music.
//!
//! # Aviso: endpoints não oficiais
//!
//! Os endpoints em `amp-api.music.apple.com/v1/me/library/*` não fazem parte
//! de uma API pública documentada pela Apple: foram mapeados observando o tráfego
//! do player web (`music.apple.com`). A Apple não oferece garantia de
//! estabilidade para eles e pode alterar caminhos, campos do JSON, regras de
//! CORS ou a autenticação a qualquer momento, sem aviso — o que quebrará o
//! Sitrus. Para limitar o estrago, todo o parsing está isolado em `models`
//! (campos ignorados não quebram) e coberto por fixtures offline em
//! `tests/fixtures/`.
//!
//! Cada requisição autentica com o bearer do player ([`crate::token`]) mais o
//! `Media-User-Token` da sessão ([`crate::session`]), responde a 429/5xx com
//! retry em backoff exponencial e interpreta a paginação pelo campo `next`.

use std::future::Future;
use std::time::Duration;

use serde::de::DeserializeOwned;
use url::Url;

use crate::error::{ApiError, ApiResult};
use crate::models::{LibraryAlbum, LibraryArtist, LibraryPlaylist, LibrarySong, Page};
use crate::session::Session;
use crate::token::{DeveloperToken, TokenProvider};

/// URL base da API interna usada pelo player web (não oficial).
pub const AMP_API_BASE: &str = "https://amp-api.music.apple.com/";
/// Origem anexada às requisições — o `amp-api` exige a origem do player.
pub(crate) const PLAYER_ORIGIN: &str = "https://music.apple.com";

/// Política de retry/backoff para respostas 429 e 5xx.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Número máximo de tentativas, contando a primeira.
    pub max_attempts: u32,
    /// Atraso da primeira espera; dobra a cada tentativa posterior.
    pub initial_delay: Duration,
    /// Teto do atraso calculado (não limita um `Retry-After` maior).
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(8),
        }
    }
}

impl RetryPolicy {
    /// Verdadeiro para os status que merecem nova tentativa: 429 e 5xx.
    #[must_use]
    pub fn should_retry(status: u16) -> bool {
        status == 429 || (500..=599).contains(&status)
    }

    /// Atraso antes da tentativa `attempt` (1-baseado): backoff exponencial de
    /// `initial_delay` dobrando até `max_delay`, nunca menor que um
    /// `Retry-After` recebido do servidor.
    #[must_use]
    pub fn delay_for(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        let exponent = attempt.saturating_sub(1).min(4);
        let backoff = self
            .initial_delay
            .saturating_mul(2_u32.saturating_pow(exponent))
            .min(self.max_delay);
        match retry_after {
            Some(server_hint) => backoff.max(server_hint),
            None => backoff,
        }
    }
}

/// Cliente dos endpoints de biblioteca (`/v1/me/library/*`) usados pelo player
/// web da Apple Music. **Não oficial** — veja o aviso no topo do módulo.
#[derive(Debug, Clone)]
pub struct LibraryClient {
    http: reqwest::Client,
    base: Url,
    tokens: TokenProvider,
    session: Session,
    retry: RetryPolicy,
}

impl LibraryClient {
    /// Constrói o cliente a partir do provedor de bearer e da sessão do
    /// usuário (media-user-token, cookies, storefront).
    ///
    /// # Erros
    /// [`ApiError::Network`] quando o TLS/reqwest não puder inicializar;
    /// [`ApiError::Parse`] nunca deve ocorrer (URL base é literal).
    pub fn new(tokens: TokenProvider, session: Session) -> ApiResult<Self> {
        let base = Url::parse(AMP_API_BASE)
            .map_err(|e| ApiError::Parse(format!("URL base do amp-api inválida: {e}")))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("sitrus/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base,
            tokens,
            session,
            retry: RetryPolicy::default(),
        })
    }

    /// Substitui a política padrão de retry (encadeável).
    #[must_use]
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Sessão em uso, somente leitura (media-user-token, cookies, storefront).
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Biblioteca de faixas do usuário; `offset`/`limit` paginam da primeira
    /// página.
    pub async fn library_songs(&self, offset: u32, limit: u32) -> ApiResult<Page<LibrarySong>> {
        self.library_page("songs", offset, limit).await
    }

    /// Biblioteca de artistas do usuário.
    pub async fn library_artists(&self, offset: u32, limit: u32) -> ApiResult<Page<LibraryArtist>> {
        self.library_page("artists", offset, limit).await
    }

    /// Biblioteca de álbuns do usuário.
    pub async fn library_albums(&self, offset: u32, limit: u32) -> ApiResult<Page<LibraryAlbum>> {
        self.library_page("albums", offset, limit).await
    }

    /// Biblioteca de playlists do usuário.
    pub async fn library_playlists(
        &self,
        offset: u32,
        limit: u32,
    ) -> ApiResult<Page<LibraryPlaylist>> {
        self.library_page("playlists", offset, limit).await
    }

    /// Segue o cursor `next` devolvido por uma página anterior, para paginação
    /// completa sem refazer contadores manuais.
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando a URL `next` não é absoluta ou aponta para
    /// um host diferente do `amp-api`.
    pub async fn fetch_next<T>(&self, next: &str) -> ApiResult<Page<T>>
    where
        T: DeserializeOwned,
    {
        let url = sanitize_next(&self.base, next)?;
        self.execute_page(url).await
    }

    /// Endpoint genérico `/v1/me/library/{resource}` com `offset`/`limit`.
    async fn library_page<T>(&self, resource: &str, offset: u32, limit: u32) -> ApiResult<Page<T>>
    where
        T: DeserializeOwned,
    {
        let url = library_url(&self.base, resource, offset, limit)?;
        self.execute_page(url).await
    }

    /// Envia a requisição (com retry) e interpreta a página ou mapeia o erro.
    async fn execute_page<T>(&self, url: Url) -> ApiResult<Page<T>>
    where
        T: DeserializeOwned,
    {
        let (status, retry_after, body) = self.dispatch(url).await?;
        match status {
            200 => parse_page(&body),
            other => Err(ApiError::from_status(other, &body, retry_after)),
        }
    }

    /// Monta a requisição autenticada: bearer + `Media-User-Token` + cabecalhos
    /// de origem do player.
    fn request(&self, url: Url, token: &DeveloperToken) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .get(url)
            .bearer_auth(&token.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ORIGIN, PLAYER_ORIGIN)
            .header(reqwest::header::REFERER, self.referer());
        if let Some(media_user_token) = self.session.media_user_token.as_deref() {
            request = request.header("Media-User-Token", media_user_token);
        }
        request
    }

    /// Referer do player com a loja da sessão (`https://music.apple.com/br/`).
    fn referer(&self) -> String {
        if self.session.storefront.is_empty() {
            format!("{PLAYER_ORIGIN}/")
        } else {
            format!("{PLAYER_ORIGIN}/{}/", self.session.storefront)
        }
    }

    /// Garante a pré-condição de sessão, obtém o bearer válido e executa a
    /// requisição com retry/backoff devolvendo `(status, retry_after, corpo)`.
    async fn dispatch(&self, url: Url) -> ApiResult<(u16, Option<Duration>, String)> {
        if self.session.media_user_token.is_none() {
            return Err(ApiError::Unauthorized(
                "sessão sem media_user_token; importe os cookies do player com \
                 Session::from_pasted_cookies"
                    .to_owned(),
            ));
        }
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

/// Monta a URL absoluta de `/v1/me/library/{resource}` com `offset` e `limit`.
///
/// # Erros
/// [`ApiError::Parse`] se o endpoint não resolver contra a base (impossível
/// para os recursos conhecidos; protege contra recursos com nome inválido).
pub fn library_url(base: &Url, resource: &str, offset: u32, limit: u32) -> ApiResult<Url> {
    let path = format!("v1/me/library/{resource}");
    let mut url = base
        .join(&path)
        .map_err(|e| ApiError::Parse(format!("endpoint de biblioteca inválido: {e}")))?;
    url.query_pairs_mut()
        .append_pair("offset", &offset.to_string())
        .append_pair("limit", &limit.to_string());
    Ok(url)
}

/// Valida a URL `next` antes de segui-la: precisa ser absoluta e apontar para
/// o mesmo host da base, para não seguir URLs estranhas devolvidas na resposta.
///
/// # Erros
/// [`ApiError::Parse`] nas violações acima.
pub fn sanitize_next(base: &Url, next: &str) -> ApiResult<Url> {
    let url = Url::parse(next).map_err(|e| ApiError::Parse(format!("URL `next` inválida: {e}")))?;
    if url.host_str() != base.host_str() {
        return Err(ApiError::Parse(format!(
            "URL `next` aponta para host inesperado: {url}"
        )));
    }
    Ok(url)
}

/// Interpreta o corpo JSON de uma página.
///
/// # Erros
/// [`ApiError::Parse`] com o detalhe do serde quando o JSON está truncado ou
/// não casa com [`Page`].
pub fn parse_page<T>(body: &str) -> ApiResult<Page<T>>
where
    T: DeserializeOwned,
{
    // Duas fases via `Value` para tolerar envelope sem `data` (página vazia)
    // sem empurrar bounds extras para o derive de `Page`.
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ApiError::Parse(format!("JSON da página não pôde ser interpretado: {e}")))?;
    let data = match value.get("data") {
        Some(items) => serde_json::from_value(items.clone()).map_err(|e| {
            ApiError::Parse(format!("itens da página não pôde ser interpretada: {e}"))
        })?,
        None => Vec::new(),
    };
    let next = value
        .get("next")
        .and_then(|item| item.as_str())
        .map(ToOwned::to_owned);
    Ok(Page { data, next })
}

/// Inteiro de segundos do header `Retry-After` (formato de datas HTTP não é
/// tratado: devolve `None` e o backoff próprio entra em ação).
fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

/// Executa `op` repetindo em 429/5xx conforme `policy`. Erros não-retriáveis
/// (rede, 401 e demais status) propagam sem nova tentativa; quando as
/// tentativas se esgotam, devolve a última resposta (status, `Retry-After`,
/// corpo) para o chamador classificar com [`ApiError::from_status`].
pub async fn execute_with_retry<F, Fut>(
    policy: &RetryPolicy,
    mut op: F,
) -> ApiResult<(u16, Option<Duration>, String)>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ApiResult<(u16, Option<Duration>, String)>>,
{
    let mut attempt: u32 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let (status, retry_after, body) = op().await?;
        if !RetryPolicy::should_retry(status) || attempt >= policy.max_attempts {
            return Ok((status, retry_after, body));
        }
        let delay = policy.delay_for(attempt, retry_after);
        tracing::debug!(
            status,
            attempt,
            delay_ms = delay.as_millis() as u64,
            "resposta retriável; nova tentativa após backoff"
        );
        tokio::time::sleep(delay).await;
    }
}
