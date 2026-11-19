use std::io::Cursor;
use std::num::{NonZeroU16, NonZeroU32};
use std::time::Duration;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::PlayerError;

const MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct DecodedAudio {
    pub(crate) samples: Vec<f32>,
    pub(crate) channels: NonZeroU16,
    pub(crate) sample_rate: NonZeroU32,
    pub(crate) duration: Duration,
}

pub(crate) fn map_symphonia_error(error: SymphoniaError) -> PlayerError {
    match error {
        SymphoniaError::IoError(error) => PlayerError::Io(error.to_string()),
        SymphoniaError::DecodeError(message) => PlayerError::Decode(message.to_owned()),
        SymphoniaError::SeekError(_) => {
            PlayerError::Decode("seek failure while reading the media".to_owned())
        }
        SymphoniaError::Unsupported(message) => PlayerError::Unsupported(message.to_owned()),
        SymphoniaError::LimitError(message) => {
            PlayerError::Decode(format!("decode limit reached: {message}"))
        }
        SymphoniaError::ResetRequired => {
            PlayerError::Decode("decoder requested an unsupported reset".to_owned())
        }
        other => PlayerError::Decode(other.to_string()),
    }
}

pub(crate) fn decode_audio(
    bytes: Vec<u8>,
    extension_hint: Option<&str>,
) -> Result<DecodedAudio, PlayerError> {
    let mut hint = Hint::new();
    if let Some(extension) = extension_hint {
        hint.with_extension(extension);
    }
    let media = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let probed = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(map_symphonia_error)?;
    let mut format = probed;

    let (track_id, codec_parameters) = format
        .default_track(TrackType::Audio)
        .map(|track| (track.id, track.codec_params.clone()))
        .ok_or_else(|| PlayerError::Decode("no audio track was found".to_owned()))?;
    let audio_parameters = codec_parameters
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(|| PlayerError::Decode("audio codec parameters are missing".to_owned()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_parameters, &AudioDecoderOptions::default())
        .map_err(map_symphonia_error)?;

    let mut samples = Vec::new();
    let mut stream_spec = None;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(error) => return Err(map_symphonia_error(error)),
        };
        if packet.track_id != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet).map_err(map_symphonia_error)?;
        let channels = u16::try_from(decoded.spec().channels().count())
            .map_err(|_| PlayerError::Decode("unsupported channel count".to_owned()))?;
        let sample_rate = decoded.spec().rate();
        let channels = NonZeroU16::new(channels)
            .ok_or_else(|| PlayerError::Decode("audio has no channels".to_owned()))?;
        let sample_rate = NonZeroU32::new(sample_rate)
            .ok_or_else(|| PlayerError::Decode("audio has no sample rate".to_owned()))?;
        let current_spec = (channels, sample_rate);
        if stream_spec.is_some_and(|expected| expected != current_spec) {
            return Err(PlayerError::Decode(
                "audio stream changed its channel count or sample rate".to_owned(),
            ));
        }
        stream_spec = Some(current_spec);

        let mut packet_samples: Vec<f32> = Vec::new();
        decoded.copy_to_vec_interleaved(&mut packet_samples);
        let new_len = samples
            .len()
            .checked_add(packet_samples.len())
            .ok_or_else(|| PlayerError::Decode("decoded audio size overflow".to_owned()))?;
        let new_bytes = new_len
            .checked_mul(std::mem::size_of::<f32>())
            .ok_or_else(|| PlayerError::Decode("decoded audio size overflow".to_owned()))?;
        if new_bytes > MAX_DECODED_BYTES {
            return Err(PlayerError::Decode(
                "decoded audio exceeds the 512 MiB safety limit".to_owned(),
            ));
        }
        samples.extend(packet_samples);
    }

    let (channels, sample_rate) = stream_spec.ok_or_else(|| {
        PlayerError::Decode("audio stream contains no decodable samples".to_owned())
    })?;
    let frame_count = samples.len() / usize::from(channels.get());
    let rate = u64::from(sample_rate.get());
    let frames = u64::try_from(frame_count)
        .map_err(|_| PlayerError::Decode("decoded frame count overflow".to_owned()))?;
    let duration = Duration::new(
        frames / rate,
        ((frames % rate) * 1_000_000_000 / rate) as u32,
    );

    Ok(DecodedAudio {
        samples,
        channels,
        sample_rate,
        duration,
    })
}

#[cfg(test)]
pub(crate) fn synthetic_wav_bytes() -> Vec<u8> {
    let sample_rate = 8_000_u32;
    let sample_count = 800_u32;
    let data_size = sample_count * 2;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..sample_count {
        let sample = if index % 2 == 0 {
            1_000_i16
        } else {
            -1_000_i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_symphonia_decode_error_to_player_decode_error() {
        let error = map_symphonia_error(SymphoniaError::DecodeError("invalid frame"));
        assert!(matches!(error, PlayerError::Decode(message) if message == "invalid frame"));
    }

    #[test]
    fn maps_symphonia_io_error_to_player_io_error() {
        let error = map_symphonia_error(SymphoniaError::IoError(std::io::Error::other("disk")));
        assert!(matches!(error, PlayerError::Io(message) if message == "disk"));
    }

    #[test]
    fn decodes_a_synthetic_wav_without_an_audio_device() {
        let decoded = decode_audio(synthetic_wav_bytes(), Some("wav"))
            .unwrap_or_else(|error| panic!("synthetic WAV should decode: {error}"));
        assert_eq!(decoded.channels.get(), 1);
        assert_eq!(decoded.sample_rate.get(), 8_000);
        assert_eq!(decoded.duration, Duration::from_millis(100));
        assert_eq!(decoded.samples.len(), 800);
    }
}
