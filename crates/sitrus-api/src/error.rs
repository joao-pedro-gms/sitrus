//! Erros tipados da comunicação com a API da Apple Music.

use std::time::Duration;

/// Resultado padrão das operações da API.
pub type ApiResult<T> = Result<T, ApiError>;

/// Erros possíveis ao falar com os endpoints do player web da Apple Music.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApiError {
    /// Resposta HTTP 401: token inválido, revogado ou sessão sem
    /// media-user-token.
    #[error("não autorizado (HTTP 401): {0}")]
    Unauthorized(String),

    /// Resposta HTTP 429; `retry_after` vem do cabeçalho `Retry-After`
    /// quando o servidor o envia.
    #[error("limite de requisições atingido (HTTP 429)")]
    RateLimited {
        /// Tempo sugerido pelo servidor antes de tentar de novo.
        retry_after: Option<Duration>,
    },

    /// Falha de transporte/TLS antes de obter uma resposta completa.
    #[error("erro de rede: {0}")]
    Network(#[from] reqwest::Error),

    /// Corpo recebido não é o JSON esperado (resposta truncada ou formato
    /// alterado pela Apple).
    #[error("resposta não pôde ser interpretada: {0}")]
    Parse(String),

    /// O token em cache expirou e nenhum token válido foi encontrado no
    /// bundle do player para repor o cache.
    #[error("token de desenvolvedor expirado desde {0} (epoch)")]
    TokenExpired(String),

    /// Status HTTP inesperado: 5xx depois do esgotamento das tentativas ou
    /// qualquer outro status não coberto pelas variantes acima.
    #[error("resposta HTTP inesperada (status {0}): {1}")]
    Http(u16, String),
}

impl ApiError {
    /// Mapeia um status HTTP, com o corpo bruto da resposta e o `Retry-After`
    /// já interpretado, para o erro correspondente.
    #[must_use]
    pub fn from_status(status: u16, body: &str, retry_after: Option<Duration>) -> Self {
        match status {
            401 => ApiError::Unauthorized(truncate_body(body)),
            429 => ApiError::RateLimited { retry_after },
            other => ApiError::Http(other, truncate_body(body)),
        }
    }
}

/// Corpo de erro pronto para exibição, cortado quando é muito longo.
fn truncate_body(body: &str) -> String {
    const MAX_CHARS: usize = 300;
    let trimmed = body.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_owned();
    }
    let mut cut: String = trimmed.chars().take(MAX_CHARS).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 401 vira `Unauthorized` com o corpo anexado à mensagem.
    #[test]
    fn status_401_becomes_unauthorized() {
        let error = ApiError::from_status(401, r#"{"errors":[{"title":"Unauthorized"}]}"#, None);
        assert!(matches!(error, ApiError::Unauthorized(body) if body.contains("Unauthorized")));
    }

    /// 429 vira `RateLimited` preservando o `Retry-After` recebido.
    #[test]
    fn status_429_becomes_rate_limited() {
        let error = ApiError::from_status(429, "", Some(Duration::from_secs(7)));
        assert!(
            matches!(error, ApiError::RateLimited { retry_after: Some(d) } if d == Duration::from_secs(7))
        );
    }

    /// 5xx (após esgotar as tentativas) vira `Http` com o status original.
    #[test]
    fn status_5xx_becomes_http() {
        let error = ApiError::from_status(503, "service unavailable", None);
        assert!(matches!(error, ApiError::Http(503, _)));
    }

    /// Corpos longos são cortados sem partir caracteres multibyte.
    #[test]
    fn long_bodies_are_truncated_safely() {
        let long = "ç".repeat(400);
        let error = ApiError::from_status(404, &long, None);
        let ApiError::Http(404, body) = error else {
            panic!("404 deve mapear para ApiError::Http");
        };
        assert_eq!(body.chars().count(), 301); // 300 caracteres + reticências
    }
}
