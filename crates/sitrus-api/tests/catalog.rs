//! Parsing e composição de URLs do catálogo (`api.music.apple.com`), offline
//! e com fixtures reais. O envio por HTTP está coberto por servidores de
//! loopback nos testes internos da crate; aqui nenhuma conexão é aberta.

use std::path::Path;

use sitrus_api::catalog::{
    CATALOG_API_BASE, catalog_url, parse_catalog_resource, parse_search_page, search_url,
};
use sitrus_api::client::{parse_page, sanitize_next};
use sitrus_api::error::ApiError;
use sitrus_api::models::{CatalogAlbum, CatalogArtist, CatalogPlaylist, CatalogSong, Page};
use url::Url;

/// Lê um fixture de `tests/fixtures/`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {name} não pôde ser lido: {e}"))
}

/// Base do catálogo para compor e validar URLs.
fn catalog_base() -> Url {
    Url::parse(CATALOG_API_BASE).expect("URL base literal")
}

/// Fixture de busca: `name`, `artistName`, `albumName`, `durationInMillis`,
/// `previewUrl`, `playCount` e `artwork` chegam intactos; campos ausentes
/// ficam `None` em vez de quebrar o item.
#[test]
fn search_fixture_parses_catalog_songs() {
    let page: Page<CatalogSong> =
        parse_search_page(&fixture("catalog_search.json")).expect("fixture de busca é válido");
    assert_eq!(page.data.len(), 2);

    let first = &page.data[0];
    assert_eq!(first.id, "song-101");
    assert_eq!(first.attributes.name.as_deref(), Some("Ponte de Sol"));
    assert_eq!(
        first.attributes.artist_name.as_deref(),
        Some("Orquestra Sitrus")
    );
    assert_eq!(
        first.attributes.album_name.as_deref(),
        Some("Jardim Interno")
    );
    assert_eq!(first.attributes.duration_in_millis, Some(231_579));
    assert_eq!(
        first.attributes.preview_url.as_deref(),
        Some("https://audio-ssl.itunes.apple.com/preview/song-101.m4a")
    );
    assert_eq!(first.attributes.play_count, Some(1234));
    let artwork = first
        .attributes
        .artwork
        .as_ref()
        .expect("capa presente na busca");
    assert_eq!(
        artwork.url_for(300, 300).as_deref(),
        Some("https://is1-ssl.mzstatic.com/image/thumb/Music125/v4/orchard/300x300bb.jpg")
    );

    assert_eq!(page.data[1].attributes.play_count, None);
    assert!(page.data[1].attributes.artwork.is_none());
    assert!(page.data[1].attributes.name.is_some());
}

/// O cursor `next` da busca é preservado como URL absoluta do mesmo host.
#[test]
fn search_fixture_keeps_absolute_next_cursor() {
    let page: Page<CatalogSong> =
        parse_search_page(&fixture("catalog_search.json")).expect("fixture de busca é válido");
    let next = page.next.as_deref().expect("fixture tem cursor `next`");
    assert_eq!(
        next,
        "https://api.music.apple.com/v1/catalog/br/search?term=citrus&types=songs&limit=2&offset=2"
    );
    let sanitized = sanitize_next(&catalog_base(), next).expect("cursor do mesmo host é aceito");
    assert_eq!(sanitized.as_str(), next);
}

/// Envelope sem a seção `songs` devolve página vazia — busca sem resultado
/// não é erro.
#[test]
fn search_without_songs_section_is_empty_page() {
    for raw in [
        "{}",
        r#"{"results":{}}"#,
        r#"{"results":{"albums":{"next":"https://api.music.apple.com/v1/x"}}}"#,
    ] {
        let page = parse_search_page(raw).expect("envelope tolerado");
        assert!(page.data.is_empty(), "raw: {raw}");
        assert!(page.next.is_none(), "raw: {raw}");
    }
}

/// Álbum com include=tracks: `trackCount`, `releaseDate`, `artwork` e as
/// faixas no relacionamento (com cursor `next` preservado).
#[test]
fn album_fixture_parses_attributes_and_track_relationship() {
    let album: CatalogAlbum =
        parse_catalog_resource(&fixture("catalog_album.json")).expect("fixture de álbum é válido");
    assert_eq!(album.id, "alb-1");
    assert_eq!(album.attributes.name.as_deref(), Some("Jardim Interno"));
    assert_eq!(
        album.attributes.artist_name.as_deref(),
        Some("Orquestra Sitrus")
    );
    assert_eq!(album.attributes.track_count, Some(12));
    assert_eq!(album.attributes.release_date.as_deref(), Some("2026-03-14"));
    assert!(album.attributes.artwork.is_some());

    let tracks = album
        .relationships
        .tracks
        .as_ref()
        .expect("faixas do include=tracks");
    assert_eq!(tracks.data.len(), 2);
    assert_eq!(tracks.data[0].id, "song-101");
    assert_eq!(
        tracks.data[0].attributes.preview_url.as_deref(),
        Some("https://audio-ssl.itunes.apple.com/preview/song-101.m4a")
    );
    assert_eq!(tracks.data[1].attributes.play_count, None);
    assert_eq!(
        tracks.next.as_deref(),
        Some("https://api.music.apple.com/v1/catalog/br/albums/alb-1/tracks?offset=2&limit=2")
    );
    sanitize_next(
        &catalog_base(),
        tracks.next.as_deref().expect("`next` do relacionamento"),
    )
    .expect("cursor do relacionamento passa na validação de host");
}

/// Álbum sem `relationships` parseia com relacionamento vazio — respostas sem
/// include não são erro.
#[test]
fn album_without_relationships_defaults_to_empty() {
    let album: CatalogAlbum = parse_catalog_resource(
        r#"{"data":[{"id":"alb-2","attributes":{"name":"Compilado Verão","trackCount":8}}]}"#,
    )
    .expect("álbum mínimo é válido");
    assert_eq!(album.attributes.track_count, Some(8));
    assert!(album.relationships.tracks.is_none());
}

/// Detalhe sem itens (`data` vazia ou ausente) é erro `Parse`, não panic.
#[test]
fn detail_response_without_items_is_parse_error() {
    let result: Result<CatalogAlbum, ApiError> = parse_catalog_resource("{\"data\":[]}");
    let error = result.expect_err("`data` vazia é erro");
    assert!(matches!(error, ApiError::Parse(_)));
}

/// JSON truncado vira `Parse` nas duas formas de resposta.
#[test]
fn truncated_catalog_body_is_parse_error() {
    assert!(matches!(
        parse_catalog_resource::<CatalogAlbum>("{\"data\":[{\"id\":"),
        Err(ApiError::Parse(_))
    ));
    assert!(matches!(
        parse_search_page("{\"results\":{\"songs\":{\"data\":[{\"id\""),
        Err(ApiError::Parse(_))
    ));
}

/// Artista com include=albums: nome e álbuns no relacionamento.
#[test]
fn artist_fixture_parses_album_relationship() {
    let artist: CatalogArtist = parse_catalog_resource(&fixture("catalog_artist.json"))
        .expect("fixture de artista é válido");
    assert_eq!(artist.id, "art-1");
    assert_eq!(artist.attributes.name.as_deref(), Some("Orquestra Sitrus"));

    let albums = artist
        .relationships
        .albums
        .as_ref()
        .expect("álbuns do include=albums");
    assert_eq!(albums.data.len(), 2);
    assert_eq!(albums.data[0].id, "alb-1");
    assert_eq!(
        albums.data[0].attributes.name.as_deref(),
        Some("Jardim Interno")
    );
    assert_eq!(albums.data[0].attributes.track_count, Some(12));
    assert_eq!(
        albums.data[1].attributes.name.as_deref(),
        Some("Compilado Verão")
    );
    assert!(albums.next.is_some());
}

/// Playlist com include=tracks: nome, curador, artwork e faixas.
#[test]
fn playlist_fixture_parses_artwork_and_track_relationship() {
    let playlist: CatalogPlaylist = parse_catalog_resource(&fixture("catalog_playlist.json"))
        .expect("fixture de playlist é válido");
    assert_eq!(playlist.id, "pl-1");
    assert_eq!(playlist.attributes.name.as_deref(), Some("Foco"));
    assert_eq!(
        playlist.attributes.curator_name.as_deref(),
        Some("Sitrus Editorial")
    );
    let artwork = playlist
        .attributes
        .artwork
        .as_ref()
        .expect("capa da playlist");
    assert_eq!(artwork.width, Some(2000));
    assert!(
        artwork
            .url_for(400, 400)
            .as_deref()
            .is_some_and(|url| url.ends_with("400x400bb.jpg")),
        "template com placeholders resolve: {:?}",
        artwork.url_for(400, 400)
    );

    let tracks = playlist
        .relationships
        .tracks
        .as_ref()
        .expect("faixas do include=tracks");
    assert_eq!(tracks.data.len(), 2);
    assert_eq!(tracks.data[0].id, "song-101");
    assert!(
        tracks.data[0]
            .attributes
            .preview_url
            .as_deref()
            .is_some_and(|url| url.ends_with("song-101.m4a")),
        "faixa de playlist também tem previewUrl"
    );
    assert_eq!(
        tracks.next.as_deref(),
        Some("https://api.music.apple.com/v1/catalog/br/playlists/pl-1/tracks?offset=2&limit=2")
    );
}

/// Faixas do artista: coleção `data` com cursor `next` (parse genérico de
/// página, como no resto da crate).
#[test]
fn artist_songs_fixture_parses_collection_page() {
    let page: Page<CatalogSong> =
        parse_page(&fixture("catalog_artist_songs.json")).expect("fixture de coleção é válido");
    assert_eq!(page.data.len(), 2);
    assert_eq!(page.data[0].id, "song-101");
    assert_eq!(page.data[1].attributes.play_count, Some(77));

    let next = page.next.as_deref().expect("fixture tem cursor `next`");
    assert_eq!(
        next,
        "https://api.music.apple.com/v1/catalog/br/artists/art-1/songs?offset=2&limit=2"
    );
    sanitize_next(&catalog_base(), next).expect("cursor do mesmo host é aceito");
}

/// As URLs do cliente compõem `v1/catalog/{storefront}/...` e a query na
/// ordem documentada, com percent-encoding do `term`.
#[test]
fn catalog_urls_compose_path_and_query() {
    let base = catalog_base();

    let search = search_url(&base, "br", "citrus", "songs", 25).expect("URL de busca compõe");
    assert_eq!(
        search.as_str(),
        "https://api.music.apple.com/v1/catalog/br/search?term=citrus&types=songs&limit=25"
    );

    // `query_pairs_mut` usa serialização form-urlencoded: espaço vira `+`
    // (o servidor interpreta ambos como espaço).
    let spaced = search_url(&base, "us", "new york", "songs", 10).expect("termo com espaço");
    assert_eq!(
        spaced.as_str(),
        "https://api.music.apple.com/v1/catalog/us/search?term=new+york&types=songs&limit=10"
    );

    let songs = catalog_url(&base, "br", "artists/art-1/songs").expect("URL de músicas");
    assert_eq!(
        songs.as_str(),
        "https://api.music.apple.com/v1/catalog/br/artists/art-1/songs"
    );

    let albums = catalog_url(&base, "br", "artists/art-1/albums").expect("URL de álbuns");
    assert_eq!(
        albums.as_str(),
        "https://api.music.apple.com/v1/catalog/br/artists/art-1/albums"
    );
}

/// 401 (fixture real de envelope de erro) → `Unauthorized` — o mesmo
/// mapeamento usado pelo envio de catálogo.
#[test]
fn unauthorized_catalog_response_maps_to_unauthorized() {
    let error = ApiError::from_status(401, &fixture("unauthorized.json"), None);
    let ApiError::Unauthorized(message) = error else {
        panic!("401 deve mapear para ApiError::Unauthorized");
    };
    assert!(
        message.contains("Music User Token"),
        "corpo anexado: {message}"
    );
}
