use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use url::Url;

use crate::PlayerError;
use crate::decode::{DecodedAudio, decode_audio};

const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn load_source(source: &str) -> Result<DecodedAudio, PlayerError> {
    if let Ok(url) = Url::parse(source) {
        return match url.scheme() {
            "http" | "https" => load_http(&url),
            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|()| PlayerError::InvalidSource("invalid file URL".to_owned()))?;
                load_local(&path)
            }
            _ => Err(PlayerError::InvalidSource(
                "only local files and HTTP(S) HLS playlists are supported".to_owned(),
            )),
        };
    }
    load_local(Path::new(source))
}

fn load_local(path: &Path) -> Result<DecodedAudio, PlayerError> {
    let metadata = fs::metadata(path).map_err(|error| PlayerError::Io(error.to_string()))?;
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(PlayerError::Decode(
            "source exceeds the 512 MiB safety limit".to_owned(),
        ));
    }
    let file = fs::File::open(path).map_err(|error| PlayerError::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| PlayerError::Io(error.to_string()))?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(PlayerError::Decode(
            "source exceeds the 512 MiB safety limit".to_owned(),
        ));
    }
    let extension = path.extension().and_then(|value| value.to_str());
    decode_audio(bytes, extension)
}

#[cfg(feature = "hls")]
fn load_http(url: &Url) -> Result<DecodedAudio, PlayerError> {
    use crate::hls::{Playlist, select_variant};

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(map_request_error)?;
    let mut playlist_url = url.clone();
    let mut playlist = fetch_playlist(&client, &playlist_url)?;
    if let Playlist::Master(variants) = playlist {
        let variant = select_variant(&variants)
            .ok_or_else(|| PlayerError::Playlist("master playlist has no variants".to_owned()))?;
        playlist_url = variant.uri.clone();
        playlist = fetch_playlist(&client, &playlist_url)?;
    }
    let Playlist::Media(media) = playlist else {
        return Err(PlayerError::Playlist(
            "nested HLS master playlists are not supported".to_owned(),
        ));
    };

    let uses_fmp4 = media.init_segment.is_some();
    let playlist_duration = media.duration;
    if !uses_fmp4
        && media.segments.iter().any(|segment| {
            let path = segment.uri.path().to_ascii_lowercase();
            !(path.ends_with(".aac") || path.ends_with(".adts"))
        })
    {
        return Err(PlayerError::Playlist(
            "HLS segments must be ADTS AAC (.aac/.adts) or fMP4 with EXT-X-MAP".to_owned(),
        ));
    }

    let mut combined = Vec::new();
    if let Some(init_segment) = media.init_segment {
        append_bounded(&mut combined, fetch_bytes(&client, &init_segment)?)?;
    }
    for segment in media.segments {
        append_bounded(&mut combined, fetch_bytes(&client, &segment.uri)?)?;
    }
    let extension = if uses_fmp4 { "mp4" } else { "aac" };
    let mut decoded = decode_audio(combined, Some(extension))?;
    decoded.duration = playlist_duration;
    Ok(decoded)
}

#[cfg(feature = "hls")]
fn fetch_playlist(
    client: &reqwest::blocking::Client,
    url: &Url,
) -> Result<crate::hls::Playlist, PlayerError> {
    let bytes = fetch_bytes(client, url)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| PlayerError::Playlist("playlist is not valid UTF-8".to_owned()))?;
    crate::hls::parse_playlist(url, &text).map_err(PlayerError::Playlist)
}

#[cfg(feature = "hls")]
fn fetch_bytes(client: &reqwest::blocking::Client, url: &Url) -> Result<Vec<u8>, PlayerError> {
    let response = client.get(url.clone()).send().map_err(map_request_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(PlayerError::Network(format!(
            "HTTP request failed with status {status}"
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SOURCE_BYTES as u64)
    {
        return Err(PlayerError::Network(
            "HTTP response exceeds the 512 MiB safety limit".to_owned(),
        ));
    }

    let mut limited = response.take(MAX_SOURCE_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| PlayerError::Network(error.to_string()))?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(PlayerError::Network(
            "HTTP response exceeds the 512 MiB safety limit".to_owned(),
        ));
    }
    Ok(bytes)
}

#[cfg(feature = "hls")]
fn append_bounded(target: &mut Vec<u8>, bytes: Vec<u8>) -> Result<(), PlayerError> {
    let new_len = target
        .len()
        .checked_add(bytes.len())
        .ok_or_else(|| PlayerError::Network("HLS stream size overflow".to_owned()))?;
    if new_len > MAX_SOURCE_BYTES {
        return Err(PlayerError::Network(
            "concatenated HLS stream exceeds the 512 MiB safety limit".to_owned(),
        ));
    }
    target.extend(bytes);
    Ok(())
}

#[cfg(feature = "hls")]
fn map_request_error(error: reqwest::Error) -> PlayerError {
    PlayerError::Network(error.without_url().to_string())
}

#[cfg(not(feature = "hls"))]
fn load_http(_url: &Url) -> Result<DecodedAudio, PlayerError> {
    let _ = REQUEST_TIMEOUT;
    Err(PlayerError::Unsupported(
        "HTTP playback requires the default `hls` feature".to_owned(),
    ))
}
