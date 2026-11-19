use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SendError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};

use crate::decode::DecodedAudio;
use crate::source::load_source;
use crate::{PlayerError, PlayerEvent, PlayerState};

const EVENT_TICK: Duration = Duration::from_millis(100);
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
enum Command {
    Load(String),
    Play,
    Pause,
    Seek(Duration),
    SetVolume(f32),
    Stop,
    Shutdown,
}

/// Fachada de controle. As operações enviam comandos ao worker e não fazem I/O na thread chamadora.
pub struct Player {
    command_sender: Sender<Command>,
    event_receiver: Receiver<PlayerEvent>,
}

impl Player {
    /// Cria um reprodutor e inicia a thread dedicada ao backend de áudio.
    #[must_use]
    pub fn new() -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let worker_events = event_sender.clone();
        let spawn_result = thread::Builder::new()
            .name("sitrus-audio".to_owned())
            .spawn(move || run_worker(command_receiver, worker_events));
        if let Err(error) = spawn_result {
            let _ = event_sender.send(PlayerEvent::Error(format!(
                "could not start audio worker: {error}"
            )));
        }
        Self {
            command_sender,
            event_receiver,
        }
    }

    /// Envia uma fonte local ou uma URL HTTP(S) HLS para carregamento assíncrono.
    pub fn load(&self, url: impl Into<String>) -> Result<(), PlayerError> {
        self.send(Command::Load(url.into()))
    }

    /// Inicia ou retoma a reprodução da fonte carregada.
    pub fn play(&self) -> Result<(), PlayerError> {
        self.send(Command::Play)
    }

    /// Pausa a reprodução atual.
    pub fn pause(&self) -> Result<(), PlayerError> {
        self.send(Command::Pause)
    }

    /// Solicita a posição de reprodução indicada.
    pub fn seek(&self, position: Duration) -> Result<(), PlayerError> {
        self.send(Command::Seek(position))
    }

    /// Ajusta o volume no intervalo inclusivo de 0.0 a 1.0.
    pub fn set_volume(&self, volume: f32) -> Result<(), PlayerError> {
        validate_volume(volume)?;
        self.send(Command::SetVolume(volume))
    }

    /// Interrompe a reprodução e volta à posição zero sem descartar a fonte carregada.
    pub fn stop(&self) -> Result<(), PlayerError> {
        self.send(Command::Stop)
    }

    /// Canal de eventos assíncronos emitidos pelo worker.
    #[must_use]
    pub fn events(&self) -> &Receiver<PlayerEvent> {
        &self.event_receiver
    }

    fn send(&self, command: Command) -> Result<(), PlayerError> {
        self.command_sender
            .send(command)
            .map_err(|SendError(_)| PlayerError::WorkerUnavailable)
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.command_sender.send(Command::Shutdown);
    }
}

struct PlaybackWorker {
    output: Option<MixerDeviceSink>,
    loaded: Option<DecodedAudio>,
    playback: Option<RodioPlayer>,
    events: Sender<PlayerEvent>,
    state: PlayerState,
    volume: f32,
    position: Duration,
    last_position_event: Instant,
}

impl PlaybackWorker {
    fn new(events: Sender<PlayerEvent>) -> Self {
        let output = match DeviceSinkBuilder::open_default_sink() {
            Ok(mut output) => {
                output.log_on_drop(false);
                Some(output)
            }
            Err(error) => {
                emit(
                    &events,
                    PlayerEvent::Error(format!("audio output is unavailable: {error}")),
                );
                None
            }
        };
        Self {
            output,
            loaded: None,
            playback: None,
            events,
            state: PlayerState::Stopped,
            volume: 1.0,
            position: Duration::ZERO,
            last_position_event: Instant::now(),
        }
    }

    fn process(&mut self, command: Command) -> bool {
        match command {
            Command::Load(url) => self.load(&url),
            Command::Play => self.play(),
            Command::Pause => self.pause(),
            Command::Seek(position) => self.seek(position),
            Command::SetVolume(volume) => self.set_volume(volume),
            Command::Stop => self.stop(),
            Command::Shutdown => return false,
        }
        true
    }

    fn load(&mut self, url: &str) {
        self.stop_playback();
        self.loaded = None;
        self.position = Duration::ZERO;
        self.transition(PlayerState::Stopped);
        emit(&self.events, PlayerEvent::Position(Duration::ZERO));

        match load_source(url) {
            Ok(audio) => {
                emit(&self.events, PlayerEvent::Duration(audio.duration));
                self.loaded = Some(audio);
            }
            Err(error) => emit(&self.events, PlayerEvent::Error(error.to_string())),
        }
    }

    fn play(&mut self) {
        if self.loaded.is_none() {
            emit(
                &self.events,
                PlayerEvent::Error(PlayerError::NoAudioLoaded.to_string()),
            );
            return;
        }
        let Some(output) = self.output.as_ref() else {
            emit(
                &self.events,
                PlayerEvent::Error(PlayerError::AudioOutputUnavailable.to_string()),
            );
            return;
        };

        let needs_restart = self.playback.as_ref().is_none_or(RodioPlayer::empty);
        if needs_restart {
            self.playback = None;
            let Some(audio) = self.loaded.as_ref() else {
                emit(
                    &self.events,
                    PlayerEvent::Error(PlayerError::NoAudioLoaded.to_string()),
                );
                return;
            };
            let source =
                SamplesBuffer::new(audio.channels, audio.sample_rate, audio.samples.clone());
            let playback = RodioPlayer::connect_new(output.mixer());
            playback.set_volume(self.volume);
            playback.append(source);
            let seek_result =
                (self.position > Duration::ZERO).then(|| playback.try_seek(self.position));
            if let Some(Err(error)) = seek_result {
                emit(
                    &self.events,
                    PlayerEvent::Error(PlayerError::Seek(error.to_string()).to_string()),
                );
            }
            self.playback = Some(playback);
        }

        if let Some(playback) = self.playback.as_ref() {
            playback.play();
            self.transition(PlayerState::Playing);
            self.last_position_event = Instant::now();
        }
    }

    fn pause(&mut self) {
        if let (Some(playback), PlayerState::Playing) = (self.playback.as_ref(), self.state) {
            playback.pause();
            self.position = playback.get_pos();
            self.transition(PlayerState::Paused);
            emit(&self.events, PlayerEvent::Position(self.position));
        }
    }

    fn seek(&mut self, requested: Duration) {
        let Some(audio) = self.loaded.as_ref() else {
            emit(
                &self.events,
                PlayerEvent::Error(PlayerError::NoAudioLoaded.to_string()),
            );
            return;
        };
        let position = requested.min(audio.duration);
        if let Some(Err(error)) = self
            .playback
            .as_ref()
            .map(|playback| playback.try_seek(position))
        {
            emit(
                &self.events,
                PlayerEvent::Error(PlayerError::Seek(error.to_string()).to_string()),
            );
            return;
        }
        self.position = position;
        emit(&self.events, PlayerEvent::Position(position));
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(playback) = self.playback.as_ref() {
            playback.set_volume(volume);
        }
    }

    fn stop(&mut self) {
        self.stop_playback();
        self.position = Duration::ZERO;
        self.transition(PlayerState::Stopped);
        emit(&self.events, PlayerEvent::Position(Duration::ZERO));
    }

    fn stop_playback(&mut self) {
        if let Some(playback) = self.playback.take() {
            playback.stop();
        }
    }

    fn tick(&mut self) {
        if self.state != PlayerState::Playing {
            return;
        }
        let Some(playback) = self.playback.as_ref() else {
            self.transition(PlayerState::Stopped);
            return;
        };
        if playback.empty() {
            self.position = self
                .loaded
                .as_ref()
                .map_or(Duration::ZERO, |audio| audio.duration);
            emit(&self.events, PlayerEvent::Position(self.position));
            self.playback = None;
            self.transition(PlayerState::Stopped);
            emit(&self.events, PlayerEvent::Ended);
        } else if self.last_position_event.elapsed() >= POSITION_INTERVAL {
            self.position = playback.get_pos();
            emit(&self.events, PlayerEvent::Position(self.position));
            self.last_position_event = Instant::now();
        }
    }

    fn transition(&mut self, next: PlayerState) {
        if self.state != next {
            self.state = next;
            emit(&self.events, PlayerEvent::StateChanged(next));
        }
    }
}

fn validate_volume(volume: f32) -> Result<(), PlayerError> {
    if !(0.0..=1.0).contains(&volume) {
        return Err(PlayerError::InvalidVolume(volume));
    }
    Ok(())
}

fn emit(events: &Sender<PlayerEvent>, event: PlayerEvent) {
    let _ = events.send(event);
}

fn run_worker(commands: Receiver<Command>, events: Sender<PlayerEvent>) {
    let mut worker = PlaybackWorker::new(events.clone());
    emit(&events, PlayerEvent::StateChanged(PlayerState::Stopped));
    loop {
        match commands.recv_timeout(EVENT_TICK) {
            Ok(command) => {
                if !worker.process(command) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => worker.tick(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    #[test]
    fn volume_validation_rejects_non_finite_and_out_of_range_values() {
        assert!(validate_volume(0.0).is_ok());
        assert!(validate_volume(1.0).is_ok());
        assert!(matches!(
            validate_volume(-0.01),
            Err(PlayerError::InvalidVolume(_))
        ));
        assert!(matches!(
            validate_volume(1.01),
            Err(PlayerError::InvalidVolume(_))
        ));
        assert!(matches!(
            validate_volume(f32::NAN),
            Err(PlayerError::InvalidVolume(_))
        ));
    }

    #[test]
    #[ignore = "requires an ALSA-compatible output device; build/CI hosts may not have one"]
    fn plays_a_runtime_generated_wav_on_an_audio_device() {
        let path = std::env::temp_dir().join(format!("sitrus-player-{}.wav", std::process::id()));
        fs::write(&path, crate::decode::synthetic_wav_bytes())
            .unwrap_or_else(|error| panic!("could not write synthetic WAV: {error}"));
        let player = Player::new();
        player
            .load(path.to_string_lossy().into_owned())
            .unwrap_or_else(|error| panic!("load command failed: {error}"));

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut loaded = false;
        while Instant::now() < deadline {
            match player.events().recv_timeout(Duration::from_millis(100)) {
                Ok(PlayerEvent::Duration(_)) => {
                    loaded = true;
                    break;
                }
                Ok(PlayerEvent::Error(error)) => panic!("could not load WAV: {error}"),
                Ok(_) | Err(_) => {}
            }
        }
        assert!(loaded, "timed out waiting for the synthetic WAV to load");
        player
            .play()
            .unwrap_or_else(|error| panic!("play command failed: {error}"));
        let mut playing = false;
        while Instant::now() < deadline + Duration::from_secs(1) {
            match player.events().recv_timeout(Duration::from_millis(100)) {
                Ok(PlayerEvent::StateChanged(PlayerState::Playing)) => {
                    playing = true;
                    break;
                }
                Ok(PlayerEvent::Error(error)) => panic!("could not play WAV: {error}"),
                Ok(_) | Err(_) => {}
            }
        }
        assert!(playing, "timed out waiting for playback to start");
        let _ = player.stop();
        let _ = fs::remove_file(path);
    }
}
