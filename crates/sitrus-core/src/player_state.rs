//! Máquina de estados do player, independente de backend de áudio.

use std::time::Duration;

/// Estados observáveis de reprodução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    /// Nenhuma faixa está sendo reproduzida.
    Stopped,
    /// A faixa está tocando.
    Playing,
    /// A reprodução está pausada.
    Paused,
    /// Um backend está preparando a reprodução.
    Loading,
    /// Ocorreu um erro no backend.
    Error,
}

/// Transição de estado que não pertence ao grafo permitido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("transição inválida de {from:?} para {to:?}")]
pub struct InvalidTransition {
    /// Estado anterior.
    pub from: PlaybackStatus,
    /// Estado solicitado.
    pub to: PlaybackStatus,
}

/// Estado de reprodução, temporização e volume.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    status: PlaybackStatus,
    position: Duration,
    duration: Option<Duration>,
    volume: f32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerState {
    /// Cria o player parado, no início da faixa e com volume máximo.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            position: Duration::ZERO,
            duration: None,
            volume: 1.0,
        }
    }

    /// Estado atual do player.
    #[must_use]
    pub const fn status(&self) -> PlaybackStatus {
        self.status
    }

    /// Posição atual da reprodução.
    #[must_use]
    pub const fn position(&self) -> Duration {
        self.position
    }

    /// Duração da faixa atual, quando conhecida.
    #[must_use]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Volume atual no intervalo inclusivo `0.0..=1.0`.
    #[must_use]
    pub const fn volume(&self) -> f32 {
        self.volume
    }

    /// Atualiza a duração e limita a posição atual quando necessário.
    pub fn set_duration(&mut self, duration: Option<Duration>) {
        self.duration = duration;
        if let Some(duration) = duration {
            self.position = self.position.min(duration);
        }
    }

    /// Ajusta o volume para `0.0..=1.0`; NaN é tratado como silêncio.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
    }

    /// Aplica uma transição permitida da máquina de estados.
    pub fn transition_to(&mut self, next: PlaybackStatus) -> Result<(), InvalidTransition> {
        let valid = match self.status {
            PlaybackStatus::Stopped => matches!(next, PlaybackStatus::Loading),
            PlaybackStatus::Loading => matches!(
                next,
                PlaybackStatus::Playing
                    | PlaybackStatus::Paused
                    | PlaybackStatus::Stopped
                    | PlaybackStatus::Error
            ),
            PlaybackStatus::Playing => matches!(
                next,
                PlaybackStatus::Paused
                    | PlaybackStatus::Stopped
                    | PlaybackStatus::Loading
                    | PlaybackStatus::Error
            ),
            PlaybackStatus::Paused => matches!(
                next,
                PlaybackStatus::Playing
                    | PlaybackStatus::Stopped
                    | PlaybackStatus::Loading
                    | PlaybackStatus::Error
            ),
            PlaybackStatus::Error => {
                matches!(next, PlaybackStatus::Stopped | PlaybackStatus::Loading)
            }
        };

        if !valid {
            return Err(InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        Ok(())
    }

    /// Avança a posição enquanto toca; retorna `true` uma vez ao atingir o fim.
    pub fn tick(&mut self, elapsed: Duration) -> bool {
        if self.status != PlaybackStatus::Playing {
            return false;
        }

        self.position = self.position.saturating_add(elapsed);
        match self.duration {
            Some(duration) if self.position >= duration => {
                self.position = duration;
                self.status = PlaybackStatus::Stopped;
                true
            }
            _ => false,
        }
    }
}
