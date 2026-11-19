//! Mapeamento de status HTTP e corpos de erro, offline, com fixtures.

use std::path::Path;
use std::time::Duration;

use sitrus_api::client::{RetryPolicy, parse_page};
use sitrus_api::error::ApiError;
use sitrus_api::models::{LibrarySong, Page};

/// Lê um fixture de `tests/fixtures/`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {name} não pôde ser lido: {e}"))
}

/// Resposta 401 do amp-api (fixture real de envelope de erro) vira
/// `Unauthorized` com o `detail` da Apple anexado.
#[test]
fn unauthorized_fixture_maps_to_unauthorized() {
    let body = fixture("unauthorized.json");
    let error = ApiError::from_status(401, &body, None);
    let ApiError::Unauthorized(message) = error else {
        panic!("401 deve mapear para ApiError::Unauthorized");
    };
    assert!(
        message.contains("Music User Token"),
        "corpo anexado: {message}"
    );
}

/// 401 também não deve ser retriado — a política só repete 429/5xx.
#[test]
fn unauthorized_status_is_not_retryable() {
    assert!(matches!(
        ApiError::from_status(401, "{}", None),
        ApiError::Unauthorized(_)
    ));
    assert!(!RetryPolicy::should_retry(401));
}

/// 429 devolve `RateLimited`, preservando o `Retry-After` recebido.
#[test]
fn rate_limited_status_maps_to_rate_limited() {
    let error = ApiError::from_status(429, "", Some(Duration::from_secs(9)));
    let ApiError::RateLimited { retry_after } = error else {
        panic!("429 deve mapear para ApiError::RateLimited");
    };
    assert_eq!(retry_after, Some(Duration::from_secs(9)));
}

/// 5xx esgotado vira `Http` com o status original.
#[test]
fn server_error_maps_to_http_variant() {
    let error = ApiError::from_status(502, "bad gateway", None);
    assert!(matches!(error, ApiError::Http(502, _)));
}

/// A política de retry cobre exatamente 429 e a faixa 5xx.
#[test]
fn retry_policy_targets_429_and_5xx() {
    for status in [200, 301, 401, 403, 404, 418] {
        assert!(
            !RetryPolicy::should_retry(status),
            "não deve repetir {status}"
        );
    }
    assert!(!RetryPolicy::should_retry(499));
    assert!(RetryPolicy::should_retry(429));
    for status in [500, 502, 503, 504, 599] {
        assert!(RetryPolicy::should_retry(status), "deve repetir {status}");
    }
    assert!(!RetryPolicy::should_retry(600));
}

/// O backoff dobra por tentativa, respeita o teto e não fica menor que um
/// `Retry-After` explícito do servidor.
#[test]
fn retry_backoff_grows_and_honors_retry_after() {
    let policy = RetryPolicy {
        max_attempts: 5,
        initial_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(8),
    };
    assert_eq!(policy.delay_for(1, None), Duration::from_secs(1));
    assert_eq!(policy.delay_for(2, None), Duration::from_secs(2));
    assert_eq!(policy.delay_for(3, None), Duration::from_secs(4));
    assert_eq!(policy.delay_for(4, None), Duration::from_secs(8));
    assert_eq!(policy.delay_for(9, None), Duration::from_secs(8), "teto");
    assert_eq!(
        policy.delay_for(1, Some(Duration::from_secs(30))),
        Duration::from_secs(30),
        "Retry-After do servidor vence"
    );
}

/// Resposta truncada (fixture em JSON incompleto) vira `Parse`, não panic.
#[test]
fn truncated_response_maps_to_parse_error() {
    let body = fixture("truncated.json");
    let result: Result<Page<LibrarySong>, ApiError> = parse_page::<LibrarySong>(&body);
    let error = result.expect_err("JSON truncado não pode ser interpretado");
    let ApiError::Parse(message) = error else {
        panic!("resposta truncada deve mapear para ApiError::Parse");
    };
    assert!(
        message.contains("JSON da página"),
        "detalhe presente: {message}"
    );
}

/// Corpo cujo tipo não casa com o esperado também é `Parse`.
#[test]
fn mismatched_body_maps_to_parse_error() {
    let result: Result<Page<LibrarySong>, ApiError> =
        parse_page::<LibrarySong>("{\"data\":[{\"id\":42}]}");
    assert!(matches!(result, Err(ApiError::Parse(_))));
}
