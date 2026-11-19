//! Sessão: parse de cookies colados, persistência 0600 e leitura do arquivo.

use std::fs;
use std::path::PathBuf;

use sitrus_api::session::Session;

/// Diretório temporário único por teste.
fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("sitrus-api-sessions-{}-{tag}", std::process::id()))
}

/// `Session::from_pasted_cookies` aceita o header `Cookie` de devtools: extrai
/// o media-user-token (variações de nome/caixa), a loja do cookie `geo` e
/// mantém todos os pares.
#[test]
fn from_pasted_cookies_extracts_token_storefront_and_pairs() {
    let pasted = "Cookie: MEDIA_USER_TOKEN=mtok-abc123; geo=BR; mp_tra=tra-9";
    let session = Session::from_pasted_cookies(pasted);
    assert_eq!(session.media_user_token.as_deref(), Some("mtok-abc123"));
    assert_eq!(session.storefront, "br");
    assert_eq!(
        session.cookies.get("mp_tra").map(String::as_str),
        Some("tra-9")
    );
    assert_eq!(session.cookies.len(), 3);
}

/// Variações de nome do media user token (`media-user-token`, `mediaUserToken`)
/// e colagem multiline sem o prefixo `Cookie:`.
#[test]
fn from_pasted_cookies_is_flexible_about_names_and_layout() {
    let session = Session::from_pasted_cookies(
        "media-user-token = outro-tok\ndesconhecido\ncountry=; geo=US\n",
    );
    assert_eq!(session.media_user_token.as_deref(), Some("outro-tok"));
    assert_eq!(session.storefront, "us");
    assert!(session.cookies.contains_key("country"));
    assert!(!session.cookies.contains_key("desconhecido"));

    let camel = Session::from_pasted_cookies("mediaUserToken=tok-3; geo=JP");
    assert_eq!(camel.media_user_token.as_deref(), Some("tok-3"));
    assert_eq!(camel.storefront, "jp");
}

/// Colagem vazia ou sem pares devolve sessão zerada (default).
#[test]
fn from_pasted_cookies_tolerates_empty_input() {
    let empty = Session::from_pasted_cookies("");
    assert_eq!(empty, Session::default());
    assert!(empty.media_user_token.is_none());

    let junk = Session::from_pasted_cookies("sem igual aqui;;;==");
    assert_eq!(junk, Session::default());
}

/// `cookie_header` reconstrói o valor do header na forma `nome=valor; ...`.
#[test]
fn cookie_header_rebuilds_pairs() {
    let session = Session::from_pasted_cookies("geo=BR; MEDIA_USER_TOKEN=mtok-1");
    let header = session.cookie_header().expect("há cookies");
    assert!(header.contains("MEDIA_USER_TOKEN=mtok-1"));
    assert!(header.contains("geo=BR"));
    assert!(header.contains("; "), "pares separados por `; `");
    assert!(Session::default().cookie_header().is_none());
}

/// `save_to`/`load_from` fazem ida-e-volta idêntica e o arquivo sai com
/// permissão 0600.
#[test]
#[cfg(unix)]
fn save_and_load_roundtrip_with_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("roundtrip");
    let path = dir.join("session.json");
    let session = Session {
        media_user_token: Some("mtok-rótulo".to_owned()),
        ..Session::from_pasted_cookies("geo=BR; mp_tra=x")
    };

    session.save_to(&path).expect("gravação local");
    assert!(path.exists(), "arquivo gravado");

    let mode = fs::metadata(&path)
        .expect("arquivo existe")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "permissão exata 0600 no arquivo");

    let loaded = Session::load_from(&path).expect("leitura local");
    assert_eq!(loaded, Some(session));

    let _ = fs::remove_dir_all(&dir);
}

/// Sessão inexistente devolve `Ok(None)` — primeira execução do app.
#[test]
fn load_missing_file_is_none() {
    let dir = temp_dir("missing");
    let path = dir.join("session.json");
    assert_eq!(Session::load_from(&path).expect("ausente não é erro"), None);
    let _ = fs::remove_dir_all(&dir);
}

/// JSON corrompido (arquivo truncado/alterado) devolve erro `InvalidData`.
#[test]
fn corrupt_session_file_is_invalid_data_error() {
    let dir = temp_dir("corrupt");
    let path = dir.join("session.json");
    fs::create_dir_all(&dir).expect("dir temporário");
    fs::write(&path, "{ \"media_user_token\": \"quebrado-").expect("escreve lixo");
    let error = Session::load_from(&path).expect_err("JSON corrompido é erro");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let _ = fs::remove_dir_all(&dir);
}

/// Sobrescrever um arquivo legado (permissões frouxas) o aperta para 0600
/// antes de escrever, sem depender da criação nova.
#[test]
#[cfg(unix)]
fn rewriting_existing_file_tightens_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("tighten");
    let path = dir.join("session.json");
    fs::create_dir_all(&dir).expect("dir temporário");
    fs::write(&path, "{}").expect("arquivo legado");
    let mut loose = fs::metadata(&path).expect("existe").permissions();
    loose.set_mode(0o644);
    fs::set_permissions(&path, loose).expect("permissões frouxas no legado");

    Session::default()
        .save_to(&path)
        .expect("reescreve com 0600");
    let mode = fs::metadata(&path).expect("existe").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let _ = fs::remove_dir_all(&dir);
}
