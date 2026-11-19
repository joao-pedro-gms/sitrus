//! Parser for the playlist subset used by the native playback backend.

use std::time::Duration;

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Playlist {
    Master(Vec<Variant>),
    Media(MediaPlaylist),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Variant {
    pub(crate) bandwidth: u64,
    pub(crate) uri: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) uri: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaPlaylist {
    pub(crate) init_segment: Option<Url>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) duration: Duration,
}

pub(crate) fn select_variant(variants: &[Variant]) -> Option<&Variant> {
    variants.iter().max_by_key(|variant| variant.bandwidth)
}

pub(crate) fn parse_playlist(base: &Url, text: &str) -> Result<Playlist, String> {
    let content = text.trim_start_matches('\u{feff}');
    let mut lines = content.lines().map(str::trim);
    if lines.next() != Some("#EXTM3U") {
        return Err("playlist is missing the #EXTM3U header".to_owned());
    }

    let mut variants = Vec::new();
    let mut pending_bandwidth = None;
    let mut init_segment = None;
    let mut segments = Vec::new();
    let mut pending_duration = None;
    let mut duration = Duration::ZERO;

    for line in lines {
        if line.is_empty() {
            continue;
        }

        if let Some(attributes) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let bandwidth = attributes
                .split(',')
                .find_map(|attribute| {
                    let (key, value) = attribute.trim().split_once('=')?;
                    (key == "BANDWIDTH").then_some(value)
                })
                .ok_or_else(|| "variant is missing BANDWIDTH".to_owned())?
                .parse::<u64>()
                .map_err(|_| "variant BANDWIDTH is not an integer".to_owned())?;
            pending_bandwidth = Some(bandwidth);
            continue;
        }

        if let Some(attributes) = line.strip_prefix("#EXT-X-MAP:") {
            let uri =
                attribute_uri(attributes).ok_or_else(|| "#EXT-X-MAP is missing URI".to_owned())?;
            let resolved = base
                .join(uri)
                .map_err(|_| "#EXT-X-MAP URI is invalid".to_owned())?;
            if init_segment
                .as_ref()
                .is_some_and(|existing: &Url| existing != &resolved)
            {
                return Err("playlist changes fMP4 init segment; this is unsupported".to_owned());
            }
            init_segment = Some(resolved);
            continue;
        }

        if let Some(value) = line.strip_prefix("#EXTINF:") {
            let seconds = value
                .split_once(',')
                .map_or(value, |(duration, _)| duration)
                .trim();
            pending_duration = Some(parse_duration(seconds)?);
            continue;
        }

        if let Some(value) = line.strip_prefix("#EXT-X-KEY:") {
            let method = value
                .split(',')
                .find_map(|attribute| {
                    let (key, value) = attribute.trim().split_once('=')?;
                    (key == "METHOD").then_some(value.trim_matches('"'))
                })
                .unwrap_or("NONE");
            if method != "NONE" {
                return Err("encrypted HLS playlists are not supported".to_owned());
            }
            continue;
        }

        if line.starts_with("#EXT-X-BYTERANGE:") {
            return Err("HLS byte-range segments are not supported".to_owned());
        }

        if line.starts_with('#') {
            continue;
        }

        if let Some(bandwidth) = pending_bandwidth.take() {
            let uri = base
                .join(line)
                .map_err(|_| "variant playlist URI is invalid".to_owned())?;
            variants.push(Variant { bandwidth, uri });
        } else if let Some(segment_duration) = pending_duration.take() {
            let uri = base
                .join(line)
                .map_err(|_| "segment URI is invalid".to_owned())?;
            duration = duration
                .checked_add(segment_duration)
                .ok_or_else(|| "playlist duration overflow".to_owned())?;
            segments.push(Segment { uri });
        } else {
            return Err("playlist URI has no preceding variant or EXTINF tag".to_owned());
        }
    }

    if pending_bandwidth.is_some() {
        return Err("variant playlist URI is missing".to_owned());
    }
    if pending_duration.is_some() {
        return Err("segment URI is missing after EXTINF".to_owned());
    }
    if !variants.is_empty() {
        return Ok(Playlist::Master(variants));
    }
    if segments.is_empty() {
        return Err("media playlist contains no segments".to_owned());
    }

    Ok(Playlist::Media(MediaPlaylist {
        init_segment,
        segments,
        duration,
    }))
}

fn attribute_uri(attributes: &str) -> Option<&str> {
    let uri_start = attributes.find("URI=")? + "URI=".len();
    let value = attributes.get(uri_start..)?.trim_start();
    if let Some(quoted) = value.strip_prefix('"') {
        quoted.split_once('"').map(|(uri, _)| uri)
    } else {
        Some(value.split(',').next()?.trim())
    }
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = seconds
        .parse::<u64>()
        .map_err(|_| "EXTINF duration is invalid".to_owned())?;
    if fraction.len() > 9 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("EXTINF fractional duration is invalid".to_owned());
    }
    let mut nanos_text = fraction.to_owned();
    while nanos_text.len() < 9 {
        nanos_text.push('0');
    }
    let nanos = if nanos_text.is_empty() {
        0
    } else {
        nanos_text
            .parse::<u32>()
            .map_err(|_| "EXTINF fractional duration is invalid".to_owned())?
    };
    Ok(Duration::new(seconds, nanos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variants_and_selects_highest_bandwidth() {
        let base = Url::parse("https://cdn.example.test/music/master.m3u8")
            .unwrap_or_else(|error| panic!("fixture URL should parse: {error}"));
        let parsed = parse_playlist(&base, include_str!("../tests/fixtures/master.m3u8"))
            .unwrap_or_else(|error| panic!("master playlist should parse: {error}"));
        let Playlist::Master(variants) = parsed else {
            panic!("expected master playlist");
        };
        assert_eq!(variants.len(), 2);
        let selected = select_variant(&variants).unwrap_or_else(|| panic!("master has variants"));
        assert_eq!(selected.bandwidth, 256_000);
        assert_eq!(
            selected.uri.as_str(),
            "https://cdn.example.test/music/high/stream.m3u8"
        );
    }

    #[test]
    fn parses_segments_durations_and_fmp4_init_segment() {
        let base = Url::parse("https://cdn.example.test/music/high/stream.m3u8")
            .unwrap_or_else(|error| panic!("fixture URL should parse: {error}"));
        let parsed = parse_playlist(&base, include_str!("../tests/fixtures/fmp4.m3u8"))
            .unwrap_or_else(|error| panic!("media playlist should parse: {error}"));
        let Playlist::Media(playlist) = parsed else {
            panic!("expected media playlist");
        };
        assert_eq!(playlist.segments.len(), 2);
        assert_eq!(playlist.duration, Duration::from_millis(7_750));
        assert_eq!(
            playlist.init_segment.as_ref().map(Url::as_str),
            Some("https://cdn.example.test/music/high/init.mp4")
        );
        assert_eq!(
            playlist.segments[0].uri.as_str(),
            "https://cdn.example.test/music/high/segment-001.m4s"
        );
    }
}
