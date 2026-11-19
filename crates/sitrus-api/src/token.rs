//! Descoberta do token de desenvolvedor (bearer) do player web da Apple.
//!
//! O player em `music.apple.com` carrega um JWT de desenvolvedor embutido no
//! bundle JS público. Este módulo baixa a página do player, extrai todos os
//! candidatos a JWT (e as URLs dos assets `.js`) por **regex**, valida a
//! expiração declarada no payload (`exp`, em segundos do epoch) e cacheia o
//! token com margem de segurança. [`TokenProvider::refresh`] força a renovação;
//! [`TokenProvider::token`] usa o cache enquanto ele estiver válido.

use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use regex::Regex;
use url::Url;

use crate::error::{ApiError, ApiResult};

/// URL do player web de onde o bundle com o bearer é derivado.
pub const WEB_PLAYER_URL: &str = "https://music.apple.com/";

/// JWT de três segmentos em base64url, como aparece no HTML/bundle do player.
const JWT_PATTERN: &str = r"eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}";
/// Referência a script em HTML (`src=`/`href=`) apontando para um `.js`.
const ASSET_PATTERN: &str = r#"(?:src|href)\s*=\s*["']([^"']+\.js)["']"#;
/// Máximo de assets `.js` baixados numa mesma descoberta do token.
const MAX_ASSETS: usize = 12;
/// O token é tratado como expirado esta margem antes do `exp` real.
const EXPIRY_MARGIN_SECS: i64 = 60;

/// Regex dos candidatos a JWT, compilada uma vez por processo.
fn jwt_regex() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(JWT_PATTERN).expect("regex de JWT é constante válida"))
}

/// Regex de assets `.js` do HTML do player, compilada uma vez por processo.
fn asset_regex() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(ASSET_PATTERN).expect("regex de assets é constante válida"))
}

/// Segundos desde o epoch Unix, usados para validar o `exp` do token.
#[must_use]
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |delta| delta.as_secs() as i64)
}

/// Token bearer pronto para uso, com a expiração declarada no payload JWT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeveloperToken {
    /// JWT bruto, enviado no cabeçalho `Authorization` como Bearer.
    pub token: String,
    /// `exp` do payload, em segundos desde o epoch Unix.
    pub expires_at: i64,
}

impl DeveloperToken {
    /// Interpreta um JWT e extrai `exp` do payload (base64url sem padding).
    ///
    /// # Erros
    /// [`ApiError::Parse`] quando o token não tem payload, o payload não está
    /// em base64url, não é JSON ou não possui o campo `exp`.
    pub fn from_jwt(jwt: &str) -> ApiResult<Self> {
        let payload = jwt
            .split('.')
            .nth(1)
            .ok_or_else(|| ApiError::Parse("token sem payload".to_owned()))?;
        let bytes = base64url_decode(payload)
            .ok_or_else(|| ApiError::Parse("payload do token não está em base64url".to_owned()))?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ApiError::Parse(format!("payload do token não é JSON válido: {e}")))?;
        let expires_at = json
            .get("exp")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| ApiError::Parse("payload do token não possui campo `exp`".to_owned()))?;
        Ok(Self {
            token: jwt.to_owned(),
            expires_at,
        })
    }

    /// Verdadeiro quando o token já venceu, considerando a margem de segurança.
    #[must_use]
    pub fn is_expired(&self, now: i64) -> bool {
        now + EXPIRY_MARGIN_SECS >= self.expires_at
    }
}

/// Decodifica base64url sem padding (RFC 4648 §5); tolera `=` final.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    for character in input.chars() {
        let value = match character {
            'A'..='Z' => character as u32 - 'A' as u32,
            'a'..='z' => character as u32 - 'a' as u32 + 26,
            '0'..='9' => character as u32 - '0' as u32 + 52,
            '-' => 62,
            '_' => 63,
            '=' => continue,
            _ => return None,
        };
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    Some(output)
}

/// Extrai todos os candidatos a JWT de um texto (HTML da página ou bundle JS),
/// preservando a ordem de aparição.
#[must_use]
pub fn extract_jwt_candidates(text: &str) -> Vec<String> {
    jwt_regex()
        .find_iter(text)
        .map(|found| found.as_str().to_owned())
        .collect()
}

/// Localiza assets `.js` referenciados no HTML e resolve URLs relativas contra
/// `base`. Duplicatas são descartadas, preservando a primeira aparição.
#[must_use]
pub fn find_asset_urls(base: &Url, html: &str) -> Vec<Url> {
    let mut found: Vec<Url> = Vec::new();
    for captures in asset_regex().captures_iter(html) {
        let Some(captured) = captures.get(1).map(|c| c.as_str().to_owned()) else {
            continue;
        };
        let Ok(resolved) = base.join(&captured) else {
            continue;
        };
        if !found.contains(&resolved) {
            found.push(resolved);
        }
    }
    found
}

/// Escolhe o primeiro candidato com token válido.
///
/// # Erros
/// [`ApiError::TokenExpired`] quando há candidatos, mas todos já venceram;
/// [`ApiError::Parse`] quando nenhum candidato é um JWT interpretável.
pub fn select_token(candidates: &[String], now: i64) -> ApiResult<DeveloperToken> {
    let mut first_expired: Option<i64> = None;
    select_internal(candidates, now, &mut first_expired).ok_or_else(|| match first_expired {
        Some(expires_at) => ApiError::TokenExpired(expires_at.to_string()),
        None => {
            ApiError::Parse("nenhum token de desenvolvedor foi encontrado no player".to_owned())
        }
    })
}

/// Corpo de [`select_token`]; registra o `exp` do primeiro token expirado
/// visto, para diferenciar "todos venceram" de "não há token".
fn select_internal(
    candidates: &[String],
    now: i64,
    first_expired: &mut Option<i64>,
) -> Option<DeveloperToken> {
    for candidate in candidates {
        let Ok(token) = DeveloperToken::from_jwt(candidate) else {
            continue;
        };
        if !token.is_expired(now) {
            return Some(token);
        }
        if first_expired.is_none() {
            *first_expired = Some(token.expires_at);
        }
    }
    None
}

/// Provedor do bearer do player web, com cache expirável e renovação sob
/// demanda.
#[derive(Debug, Clone)]
pub struct TokenProvider {
    http: reqwest::Client,
    player_url: Url,
    cache: Arc<tokio::sync::RwLock<Option<DeveloperToken>>>,
}

impl TokenProvider {
    /// Cria o provider compartilhando o cliente HTTP já configurado no app.
    #[must_use]
    pub fn new(http: reqwest::Client) -> Self {
        let player_url =
            Url::parse(WEB_PLAYER_URL).expect("WEB_PLAYER_URL é uma constante literal válida");
        Self {
            http,
            player_url,
            cache: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Bearer em cache, sem tocar na rede.
    pub async fn cached_token(&self) -> Option<DeveloperToken> {
        self.cache.read().await.clone()
    }

    /// Devolve o bearer do cache enquanto ele estiver válido; caso contrário,
    /// renova de music.apple.com. Chamadores simultâneos causam uma única
    /// descoberta (dupla checagem sob a mesma trava).
    ///
    /// # Erros
    /// Propaga os erros da descoberta: rede, respostas sem token e tokens
    /// interpretáveis ausentes ([`ApiError::TokenExpired`] só aparece aqui
    /// quando a própria Apple publica um bundle com JWT vencido).
    pub async fn token(&self) -> ApiResult<DeveloperToken> {
        // Caminho rápido: cache fresco, com trava de leitura apenas.
        if let Some(token) = self.cached_token().await
            && !token.is_expired(now_unix())
        {
            return Ok(token);
        }
        let mut cache = self.cache.write().await;
        // Rechecagem: outra tarefa pode ter renovado enquanto esperávamos.
        if let Some(token) = cache.as_ref()
            && !token.is_expired(now_unix())
        {
            return Ok(token.clone());
        }
        let fresh = self.discover_token().await?;
        *cache = Some(fresh.clone());
        Ok(fresh)
    }

    /// Força a renovação do bearer a partir do bundle atual do player.
    ///
    /// Acesso real à rede (não coberto por testes offline; o contrato é
    /// exercido por `extract_jwt_candidates` e `select_token`).
    ///
    /// # Erros
    /// Rede, página/assets sem token ou com somente JWTs vencidos.
    pub async fn refresh(&self) -> ApiResult<DeveloperToken> {
        let mut cache = self.cache.write().await;
        let fresh = self.discover_token().await?;
        *cache = Some(fresh.clone());
        tracing::debug!(expires_at = fresh.expires_at, "bearer do player renovado");
        Ok(fresh)
    }

    /// Baixa a página do player; se um token válido não estiver nela, desce os
    /// assets `.js` referenciados até encontrar um.
    async fn discover_token(&self) -> ApiResult<DeveloperToken> {
        let page = self.fetch_text(self.player_url.clone()).await?;
        let now = now_unix();
        let mut first_expired: Option<i64> = None;
        if let Some(token) =
            select_internal(&extract_jwt_candidates(&page), now, &mut first_expired)
        {
            tracing::debug!(
                expires_at = token.expires_at,
                "bearer encontrado na própria página do player"
            );
            return Ok(token);
        }
        for asset in find_asset_urls(&self.player_url, &page)
            .into_iter()
            .take(MAX_ASSETS)
        {
            tracing::debug!(asset = %asset, "procurando bearer em asset do player");
            // Um asset que falha não invalida a descoberta; tenta o próximo.
            let Ok(bundle) = self.fetch_text(asset.clone()).await else {
                continue;
            };
            if let Some(token) =
                select_internal(&extract_jwt_candidates(&bundle), now, &mut first_expired)
            {
                tracing::debug!(
                    expires_at = token.expires_at,
                    "bearer extraído do bundle JS do player"
                );
                return Ok(token);
            }
        }
        match first_expired {
            Some(expires_at) => Err(ApiError::TokenExpired(expires_at.to_string())),
            None => Err(ApiError::Parse(
                "nenhum bearer válido em music.apple.com (página e assets)".to_owned(),
            )),
        }
    }

    /// Baixa um corpo como UTF-8, propagando falhas como `Network`.
    async fn fetch_text(&self, url: Url) -> ApiResult<String> {
        Ok(self.http.get(url).send().await?.text().await?)
    }
}
