//! Parsing offline das respostas de biblioteca, com os fixtures JSON reais.

use std::path::Path;

use sitrus_api::client::{library_url, parse_page, sanitize_next};
use sitrus_api::models::{LibraryPlaylist, LibrarySong, Page};
use url::Url;

/// Lê um fixture de `tests/fixtures/`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {name} não pôde ser lido: {e}"))
}

/// `/me/library/songs`: campos `attributes.{name, artistName, albumName,
/// durationInMillis, artwork}` chegam intactos com os nomes reais do JSON.
#[test]
fn songs_fixture_parses_library_songs() {
    let page: Page<LibrarySong> =
        parse_page(&fixture("library_songs.json")).expect("fixture de canções é JSON válido");
    assert_eq!(page.data.len(), 2);

    let first = &page.data[0];
    assert_eq!(first.id, "lSong-001");
    let attributes = &first.attributes;
    assert_eq!(attributes.name.as_deref(), Some("Sombra e Água"));
    assert_eq!(attributes.artist_name.as_deref(), Some("Orquestra Sitrus"));
    assert_eq!(attributes.album_name.as_deref(), Some("Jardim Interno"));
    assert_eq!(attributes.duration_in_millis, Some(231_579));

    // Faixa sem capa devolve `None`, não erro.
    assert!(page.data[1].attributes.artwork.is_none());
}

/// O cursor `next` da página é preservado como URL absoluta válida.
#[test]
fn songs_fixture_keeps_next_cursor() {
    let page: Page<LibrarySong> =
        parse_page(&fixture("library_songs.json")).expect("fixture de canções é JSON válido");
    let next = page.next.as_deref().expect("fixture tem cursor `next`");
    assert_eq!(
        next,
        "https://amp-api.music.apple.com/v1/me/library/songs?offset=2&limit=2"
    );
    let base = Url::parse(sitrus_api::client::AMP_API_BASE).expect("base literal");
    let sanitized = sanitize_next(&base, next).expect("`next` válida para o mesmo host");
    assert_eq!(sanitized.as_str(), next);
}

/// Playlist da biblioteca: `name`/`canEdit` e página sem `next`.
#[test]
fn playlists_fixture_parses_library_playlists() {
    let page: Page<LibraryPlaylist> =
        parse_page(&fixture("library_playlists.json")).expect("fixture de playlists é JSON válido");
    assert_eq!(page.data.len(), 2);
    assert_eq!(page.data[0].id, "lPlaylist-01");
    assert_eq!(page.data[0].attributes.name.as_deref(), Some("Foco"));
    assert_eq!(page.data[0].attributes.can_edit, Some(true));
    assert_eq!(page.data[1].attributes.name.as_deref(), Some("Viagem"));
    assert_eq!(page.data[1].attributes.can_edit, Some(false));
    assert!(page.next.is_none(), "última página não tem `next`");
}

/// A URL de artwork com placeholders `{w}`/`{h}` resolve nos 3 tamanhos
/// solicitados pela UI (miniatura, média e alta).
#[test]
fn artwork_url_resolves_in_three_sizes() {
    let page: Page<LibrarySong> =
        parse_page(&fixture("library_songs.json")).expect("fixture de canções é JSON válido");
    let artwork = page.data[0]
        .attributes
        .artwork
        .as_ref()
        .expect("capa presente no fixture");
    assert_eq!(artwork.width, Some(3000));
    assert_eq!(
        artwork.url_for(100, 100).as_deref(),
        Some("https://is1-ssl.mzstatic.com/image/thumb/Music125/v4/orchard/100x100bb.jpg")
    );
    assert_eq!(
        artwork.url_for(300, 300).as_deref(),
        Some("https://is1-ssl.mzstatic.com/image/thumb/Music125/v4/orchard/300x300bb.jpg")
    );
    assert_eq!(
        artwork.url_for(1000, 1000).as_deref(),
        Some("https://is1-ssl.mzstatic.com/image/thumb/Music125/v4/orchard/1000x1000bb.jpg")
    );
}

/// Template sem placeholders é devolvido como está; sem URL, `None`.
#[test]
fn artwork_url_for_handles_missing_template() {
    let artwork = sitrus_api::models::Artwork {
        url: Some("https://example.com/fixed.jpg".to_owned()),
        width: None,
        height: None,
    };
    assert_eq!(
        artwork.url_for(64, 64).as_deref(),
        Some("https://example.com/fixed.jpg")
    );
    let empty = sitrus_api::models::Artwork {
        url: None,
        width: None,
        height: None,
    };
    assert!(empty.url_for(64, 64).is_none());
}

/// A URL montada para `/v1/me/library/{resource}` usa offset/limit na ordem e
/// host corretos.
#[test]
fn library_url_carries_offset_and_limit() {
    let base = Url::parse(sitrus_api::client::AMP_API_BASE).expect("base literal");
    let url = library_url(&base, "songs", 100, 25).expect("endpoint compõe com recursos válidos");
    assert_eq!(
        url.as_str(),
        "https://amp-api.music.apple.com/v1/me/library/songs?offset=100&limit=25"
    );
    let playlists = library_url(&base, "playlists", 0, 2).expect("playlist é recurso válido");
    assert_eq!(
        playlists.as_str(),
        "https://amp-api.music.apple.com/v1/me/library/playlists?offset=0&limit=2"
    );
}
