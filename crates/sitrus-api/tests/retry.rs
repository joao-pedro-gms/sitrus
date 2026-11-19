//! O executor de retry (transporte-agóstico) testado com operações falsas —
//! nenhuma chamada de rede aqui.

use std::time::Duration;

use sitrus_api::client::{RetryPolicy, execute_with_retry};
use sitrus_api::error::ApiError;

/// Política com esperas curtas para os testes.
fn fast_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(4),
    }
}

/// 429 é repetido até a operação devolver 200: três chamadas no total.
#[tokio::test(flavor = "current_thread")]
async fn retriable_429_is_retried_until_success() {
    let policy = fast_policy();
    let mut calls = 0_u32;
    let result = execute_with_retry(&policy, || {
        calls += 1;
        let call = calls;
        async move {
            if call < 3 {
                Ok((
                    429,
                    Some(Duration::from_millis(1)),
                    String::from("lento aí"),
                ))
            } else {
                Ok((200, None, String::from("{}")))
            }
        }
    })
    .await
    .expect("a operação termina em sucesso");
    assert_eq!(calls, 3);
    assert_eq!(result.0, 200);
}

/// 5xx também é repetido e a recuperação na terceira tentativa funciona.
#[tokio::test(flavor = "current_thread")]
async fn retriable_5xx_is_retried_until_success() {
    let policy = fast_policy();
    let mut calls = 0_u32;
    let result = execute_with_retry(&policy, || {
        calls += 1;
        let call = calls;
        async move {
            if call == 1 {
                Ok((503, None, String::new()))
            } else {
                Ok((200, None, String::new()))
            }
        }
    })
    .await
    .expect("recupera na segunda tentativa");
    assert_eq!(calls, 2);
    assert_eq!(result.0, 200);
}

/// 401 não gera nova tentativa — uma única chamada.
#[tokio::test(flavor = "current_thread")]
async fn unauthorized_is_not_retried() {
    let policy = fast_policy();
    let mut calls = 0_u32;
    let result = execute_with_retry(&policy, || {
        calls += 1;
        async { Ok::<_, ApiError>((401, None, String::from("não autorizado"))) }
    })
    .await
    .expect("resposta retornada para classificação");
    assert_eq!(calls, 1);
    assert_eq!(result.0, 401);
}

/// 429 persistente esgota `max_attempts` e devolve a última resposta, que o
/// cliente (execute_page) classifica com `ApiError::from_status`.
#[tokio::test(flavor = "current_thread")]
async fn persistent_429_exhausts_attempts_and_maps_to_rate_limited() {
    let policy = fast_policy();
    let mut calls = 0_u32;
    let last = execute_with_retry(&policy, || {
        calls += 1;
        async { Ok::<_, ApiError>((429, None, String::new())) }
    })
    .await
    .expect("esgotamento devolve a última resposta");
    assert_eq!(calls, policy.max_attempts);
    let error = ApiError::from_status(last.0, &last.2, last.1);
    assert!(matches!(error, ApiError::RateLimited { retry_after: None }));
}

/// 5xx persistente esgota as tentativas e vira `Http(status)`.
#[tokio::test(flavor = "current_thread")]
async fn persistent_5xx_exhausts_attempts_and_maps_to_http() {
    let policy = fast_policy();
    let last = execute_with_retry(&policy, || async {
        Ok::<_, ApiError>((500, None, String::from("boom")))
    })
    .await
    .expect("esgotamento devolve a última resposta");
    assert!(matches!(
        ApiError::from_status(last.0, &last.2, last.1),
        ApiError::Http(500, _)
    ));
}

/// Erros que não são respostas HTTP (ex.: falha de Parse propagada pela
/// operação) não são repetidos — uma única chamada. (Um `reqwest::Error` real
/// não pode ser construído offline; o caminho de propagação é o mesmo.)
#[tokio::test(flavor = "current_thread")]
async fn non_response_errors_propagate_without_retry() {
    let policy = fast_policy();
    let mut calls = 0_u32;
    let result = execute_with_retry(&policy, || {
        calls += 1;
        async move {
            Err::<(u16, Option<Duration>, String), _>(ApiError::Parse(String::from(
                "falha simulada na operação",
            )))
        }
    })
    .await;
    assert_eq!(calls, 1);
    assert!(matches!(result, Err(ApiError::Parse(_))));
}
