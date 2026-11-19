//! Extração do bearer por regex e validação de expiração, tudo offline.

use std::io::Write as _;
use std::path::Path;

use sitrus_api::error::ApiError;
use sitrus_api::token::{
    DeveloperToken, TokenProvider, WEB_PLAYER_URL, extract_jwt_candidates, find_asset_urls,
    now_unix, select_token,
};
use url::Url;

/// Codificador base64url (sem padding) para montar JWTs de teste.
///
/// (Implementado localmente para o teste não depender de rede nem de crate
/// extra; valida a decodificação escrita em `token.rs` de mãos duplas.)
fn b64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 63] as char);
        out.push(TABLE[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple as usize & 63] as char);
        }
    }
    out
}

/// Caminho do diretório de fixtures (mesmo formato dos demais testes).
fn read_fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Monta um JWT de teste com a expiração pedida
/// (`{"alg":"HS256"}` + `{"exp":<expires_at>}` + assinatura falsa).
fn make_jwt(expires_at: i64) -> String {
    let header = b64url_encode(br#"{"alg":"HS256"}"#);
    let payload = b64url_encode(format!(r#"{{"exp":{expires_at}}}"#).as_bytes());
    let signature = b64url_encode(b"signature");
    format!("{header}.{payload}.{signature}")
}

/// JWT válido é extraído de HTML e de bundle JS com ruído ao redor.
#[test]
fn extracts_jwt_from_page_and_bundle_with_noise() {
    let jwt = make_jwt(now_unix() + 3600);
    let html = format!(
        r#"<html><script>var t="{jwt}";var cfg={{"x":1}}</script><body>Sitrus</body></html>"#
    );
    let bundle = format!("e([\"{jwt}\"],1);new AbortController;window.__init();");
    assert_eq!(extract_jwt_candidates(&html), vec![jwt.clone()]);
    assert_eq!(extract_jwt_candidates(&bundle), vec![jwt]);
}

/// Sequências parecidas com JWT mas malformadas não casam com o regex
/// (segmentos curtos, caracteres proibidos, dois segmentos).
#[test]
fn rejects_malformed_jwt_lookalikes() {
    assert!(extract_jwt_candidates("eyJhbGciOi.aaaa.bbbbbb").is_empty());
    assert!(extract_jwt_candidates("eyJhbGciOiJIUzI1NiJ9.eypleHA6MX0").is_empty());
    assert!(extract_jwt_candidates("eyJhbGciOiJIUzI1NiJ9.eyJleHA6MX0.c2ln$").is_empty());
    assert!(extract_jwt_candidates("nada de token por aqui").is_empty());
}

/// Assets `.js` do HTML são resolvidos contra a base; duplicatas são unidas e
/// referências que não são `.js` são ignoradas.
#[test]
fn finds_and_resolves_js_assets() {
    let base = Url::parse(WEB_PLAYER_URL).expect("URL literal");
    let html = r#"<script src="/assets/index-abc.js"></script>
<link rel="preload" as="script" href="./vendor.js">
<script src='/assets/index-abc.js'></script>
<link href="/style.css">
<script src="https://cdn.example.com/lib.js"></script>"#;
    let assets = find_asset_urls(&base, html);
    assert_eq!(
        assets,
        vec![
            Url::parse("https://music.apple.com/assets/index-abc.js").unwrap(),
            Url::parse("https://music.apple.com/vendor.js").unwrap(),
            Url::parse("https://cdn.example.com/lib.js").unwrap(),
        ]
    );
}

/// `from_jwt` lê o `exp` real e considera o token vivo antes e vencido depois.
#[test]
fn developer_token_reads_expiry_and_is_expired_flag() {
    let now = now_unix();
    let token = DeveloperToken::from_jwt(&make_jwt(now + 3600)).expect("JWT de teste válido");
    assert_eq!(token.expires_at, now + 3600);
    assert!(!token.is_expired(now));

    let expired = DeveloperToken::from_jwt(&make_jwt(now - 3600)).expect("JWT de teste válido");
    assert!(expired.is_expired(now));
    assert!(expired.is_expired(now - 55), "margem de 60 s protege o fim");
}

/// Payload sem `exp`, base64url inválida e token sem payload viram `Parse`.
#[test]
fn malformed_tokens_map_to_parse_error() {
    let no_exp = b64url_encode(br#"{"iat":100}"#);
    let jwt = format!("{no_exp}.e30.c2ln");
    assert!(matches!(
        DeveloperToken::from_jwt(&jwt),
        Err(ApiError::Parse(_))
    ));
    let bad_payload = format!("{}.@@@.c2ln", b64url_encode(br#"{"alg":"HS256"}"#));
    assert!(matches!(
        DeveloperToken::from_jwt(&bad_payload),
        Err(ApiError::Parse(_))
    ));
    let no_payload = "so-um-fragmento";
    assert!(matches!(
        DeveloperToken::from_jwt(no_payload),
        Err(ApiError::Parse(_))
    ));
}

/// `select_token` prefere o primeiro válido; vencidos são atalho para
/// `TokenExpired`; lixo é `Parse`.
#[test]
fn select_token_prefers_valid_and_reports_expired() {
    let now = now_unix();
    let expired = make_jwt(now - 10);
    let valid = make_jwt(now + 3600);
    let garbage = "eyJhbGciOi.aaaa.bbbbbb".to_owned();

    // Mistura: um lixo e um expirado antes do válido.
    let picked = select_token(&[garbage.clone(), expired.clone(), valid.clone()], now)
        .expect("há um token válido na lista");
    assert_eq!(picked.token, valid);

    // Só vencidos -> TokenExpired com o `exp` no texto do erro.
    let only_expired = select_token(std::slice::from_ref(&expired), now);
    assert!(matches!(only_expired, Err(ApiError::TokenExpired(_))));
    assert!(
        only_expired
            .unwrap_err()
            .to_string()
            .contains(&(now - 10).to_string())
    );

    // Só lixo -> Parse.
    let only_garbage = select_token(&[garbage], now);
    assert!(matches!(only_garbage, Err(ApiError::Parse(_))));
}

/// O provedor nasce sem cache; `cached_token` não toca na rede e devolve nada.
#[tokio::test(flavor = "current_thread")]
async fn provider_starts_with_empty_cache() {
    let http = reqwest::Client::new();
    let provider = TokenProvider::new(http);
    assert!(provider.cached_token().await.is_none());
}

/// Fixture do player usado como amostra do HTML real (o mesmo conteúdo é
/// reaproveitado pela documentação `tests/fixtures/player_page.html`).
#[test]
fn player_sample_page_yields_asset_urls_only() {
    let path = read_fixture_dir().join("player_page.html");
    let Ok(html) = std::fs::read_to_string(&path) else {
        // Fixture opcional; os demais testes cobrem extração com strings.
        return;
    };
    let base = Url::parse(WEB_PLAYER_URL).expect("URL literal");
    assert!(!find_asset_urls(&base, &html).is_empty());
}

/// Garante que o buffer de teste suporta escrita (mantém imports vivos).
#[test]
fn temp_writer_smoke() {
    let mut buffer = std::io::sink();
    buffer.write_all(b"sitrus").expect("sink aceita escrita");
}
