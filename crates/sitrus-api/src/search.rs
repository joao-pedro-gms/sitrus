//! Cliente legado da API de catálogo (`api.music.apple.com`), herdado do
//! scaffold do workspace.
//!
//! [`AppleMusicClient::search_tracks`] continua sendo um stub de envio: URL,
//! query e parser estão prontos, mas a chamada HTTP (com bearer) fica para a
//! próxima iteração. O caminho novo e autenticado do usuário está em
//! [`crate::client`].

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sitrus_core::model::{EntityId, Track};
use url::Url;

/// URL base da API pública da Apple Music (a barra final importa para `Url::join`).
pub const API_BASE: &str = "https://api.music.apple.com/";

/// Recursos pedidos na busca; limitado a faixas nesta fase.
const SEARCH_TYPES: &str = "songs";

/// Máximo de resultados por página da busca.
const SEARCH_LIMIT: &str = "25";

/// Credenciais mínimas para falar com a API da Apple Music.
#[derive(Debug, Clone)]
pub struct ApiCredentials {
    /// Token de desenvolvedor (JWT) enviado em cada requisição.
    pub developer_token: String,
    /// Código da loja (storefront) alvo, ex.: `"us"`.
    pub storefront: String,
}

/// Cliente HTTP da API da Apple Music; TLS via rustls (sem openssl nativo).
#[derive(Debug, Clone)]
pub struct AppleMusicClient {
    /// Cliente HTTP compartilhado entre todas as chamadas.
    http: reqwest::Client,
    /// Base absoluta, já normalizada com barra final.
    base: Url,
    /// Credenciais usadas em cada chamada.
    credentials: ApiCredentials,
}

impl AppleMusicClient {
    /// Cria o cliente; falha se o TLS não estiver disponível.
    pub fn new(credentials: ApiCredentials) -> Result<Self> {
        let base = Url::parse(API_BASE).context("API_BASE literal devolve URL inválida")?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("sitrus/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("falha ao construir o cliente HTTP (reqwest)")?;
        Ok(Self {
            http,
            base,
            credentials,
        })
    }

    /// Compõe a URL do endpoint de busca para a loja configurada.
    fn search_url(&self) -> Result<Url> {
        let endpoint = format!("v1/catalog/{}/search", self.credentials.storefront);
        self.base
            .join(&endpoint)
            .context("endpoint de busca inválido")
    }

    /// Consulta ao endpoint de busca do catálogo.
    ///
    /// STUB: o envio (`send`) e o processamento da resposta entram na próxima
    /// iteração; URL, query, autenticação e parser já estão prontos aqui.
    pub async fn search_tracks(&self, term: &str) -> Result<Vec<Track>> {
        let url = self.search_url().context("URL de busca inválida")?;
        tracing::debug!(url = %url, term = %term, "preparando busca no catálogo");
        let _request = self
            .http
            .get(url)
            .query(&[
                ("term", term),
                ("types", SEARCH_TYPES),
                ("limit", SEARCH_LIMIT),
            ])
            .bearer_auth(&self.credentials.developer_token);
        anyhow::bail!("requisições HTTP ainda não implementadas nesta fase (stub)")
    }
}

/// Subconjunto da resposta JSON de busca contendo apenas faixas.
#[derive(Debug, Deserialize)]
struct SearchEnvelope {
    /// Resultado do recurso `songs`; ausente quando a busca não encontra nada.
    #[serde(default)]
    songs: Option<SongsFeed>,
}

/// Feed de faixas dentro da resposta de busca.
#[derive(Debug, Deserialize)]
struct SongsFeed {
    /// Lista de faixas retornadas pela busca.
    #[serde(default)]
    data: Vec<SongPayload>,
}

/// Payload JSON de uma faixa do catálogo.
#[derive(Debug, Deserialize)]
struct SongPayload {
    /// Identificador da faixa no catálogo.
    id: String,
    /// Atributos visíveis da faixa.
    attributes: SongAttributes,
}

/// Atributos da faixa usados para preencher o modelo de domínio.
#[derive(Debug, Deserialize)]
struct SongAttributes {
    /// Nome de exibição da faixa.
    name: String,
    /// Nome do álbum, quando presente na resposta.
    #[serde(default, rename = "albumName")]
    album_name: Option<String>,
    /// Duração declarada, em milissegundos.
    #[serde(default, rename = "durationInMillis")]
    duration_in_millis: Option<u64>,
    /// URL da prévia de áudio de 30 s.
    #[serde(default, rename = "previewUrl")]
    preview_url: Option<Url>,
}

impl From<SongPayload> for Track {
    fn from(payload: SongPayload) -> Self {
        let SongPayload { id, attributes } = payload;
        Track {
            id: EntityId::new(&id),
            title: attributes.name,
            album: EntityId::new(attributes.album_name.as_deref().unwrap_or_default()),
            duration: attributes.duration_in_millis.map(Duration::from_millis),
            preview_url: attributes.preview_url,
        }
    }
}

/// Interpreta o JSON bruto de uma resposta de busca em faixas do domínio.
///
/// Respostas sem o recurso `songs` tratam-se como "nenhum resultado".
pub fn parse_search_response(raw: &str) -> Result<Vec<Track>> {
    let parsed: SearchEnvelope =
        serde_json::from_str(raw).context("JSON de busca da Apple Music inválido")?;
    Ok(parsed
        .songs
        .map(|feed| feed.data.into_iter().map(Track::from).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrai título, duração e prévia de um corpo JSON mínimo de busca.
    #[test]
    fn parses_search_response_with_one_song() {
        let raw = r#"{
            "songs": {
                "data": [
                    {
                        "id": "1",
                        "attributes": {
                            "name": "Sitrus Song",
                            "albumName": "Orchard",
                            "durationInMillis": 30000,
                            "previewUrl": "https://example.com/preview.m4a"
                        }
                    }
                ]
            }
        }"#;
        let tracks = parse_search_response(raw).expect("JSON de exemplo é válido");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id.as_str(), "1");
        assert_eq!(tracks[0].display_title(), "Sitrus Song");
        assert_eq!(tracks[0].duration, Some(Duration::from_secs(30)));
        assert!(tracks[0].preview_url.is_some());
    }

    /// Resposta sem o recurso `songs` devolve lista vazia, não erro.
    #[test]
    fn empty_response_means_no_results() {
        let tracks = parse_search_response("{}").expect("JSON vazio é válido");
        assert!(tracks.is_empty());
    }

    /// O endpoint de busca usa a loja configurada na credencial.
    #[test]
    fn search_url_targets_configured_storefront() {
        let client = AppleMusicClient::new(ApiCredentials {
            developer_token: "token".to_owned(),
            storefront: "br".to_owned(),
        })
        .expect("cliente construído em testes");
        let url = client.search_url().expect("URL de busca válida");
        assert_eq!(
            url.as_str(),
            "https://api.music.apple.com/v1/catalog/br/search"
        );
    }
}
