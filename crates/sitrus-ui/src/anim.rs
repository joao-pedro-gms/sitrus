//! Movimento do design system: curvas de easing e durações.
//!
//! egui não traz curvas prontas; aqui ficam os helpers de 120 ms (hover),
//! 180 ms (estado) e 240 ms (transição de tela/prateleira), com a curva
//! `ease-out cúbica` única do app.

use std::time::{Duration, Instant};

/// Duração de feedback de hover.
pub const HOVER_MS: u32 = 120;
/// Duração de mudança de estado (seleção, toggle).
pub const STATE_MS: u32 = 180;
/// Duração de transição de tela/prateleira.
pub const TRANSITION_MS: u32 = 240;

/// Ease-out cúbica: rápida no começo, suave no fim, dentro de `0..=1`.
///
/// `0.0 → 0.0`, `1.0 → 1.0`, `0.5 → 0.875`.
#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Duração token em `std::time::Duration`.
#[must_use]
pub fn duration(ms: u32) -> Duration {
    Duration::from_millis(u64::from(ms))
}

/// Tween de um valor `from → to` com easing ease-out cúbica.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    started: Instant,
    duration: Duration,
    from: f32,
    to: f32,
}

impl Tween {
    /// Novo tween começando agora.
    #[must_use]
    pub fn now(from: f32, to: f32, duration_ms: u32) -> Self {
        Self {
            started: Instant::now(),
            duration: duration(duration_ms),
            from,
            to,
        }
    }

    /// Duração da animação.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Valor interpolado (com easing) em um instante.
    #[must_use]
    pub fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        let t = if self.duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f32() / self.duration.as_secs_f32()
        };
        Tween::lerp(self.from, self.to, ease_out_cubic(t))
    }

    /// Tween terminou em `now`?
    #[must_use]
    pub fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }

    /// Interpolação linear simples (sem easing), exposta para testes.
    #[must_use]
    pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
        from + (to - from) * t.clamp(0.0, 1.0)
    }
}

/// Pulso repetitivo `0..=1` para shimmer/skeleton, com fase por elemento.
///
/// `time` em segundos, `period_secs` período do ciclo.
#[must_use]
pub fn pulse(time_secs: f64, period_secs: f64, phase: f64) -> f32 {
    if period_secs <= 0.0 {
        return 0.5;
    }
    let t = (time_secs / period_secs + phase).rem_euclid(1.0);
    // Meia onda senoidal: 0 → 1 → 0 no período.
    (0.5 - 0.5 * core::f32::consts::PI * (t * core::f64::consts::TAU) as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Durações fixas do design system.
    #[test]
    fn duration_helpers() {
        assert_eq!(HOVER_MS, 120);
        assert_eq!(STATE_MS, 180);
        assert_eq!(TRANSITION_MS, 240);
        assert_eq!(duration(HOVER_MS), Duration::from_millis(120));
        assert_eq!(duration(STATE_MS), Duration::from_millis(180));
        assert_eq!(duration(TRANSITION_MS), Duration::from_millis(240));
    }

    /// A curva fixa os extremos e `0.5 → 0.875`.
    #[test]
    fn ease_out_cubic_extremes_and_midpoint() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        let mid = ease_out_cubic(0.5);
        assert!(
            (mid - 0.875).abs() < 1e-6,
            "ease-out cúbica 0.5 = 0.875, foi {mid}"
        );
    }

    /// A curva é monotônica crescente e satura entradas fora de 0..1.
    #[test]
    fn ease_out_cubic_is_monotonic_and_clamped() {
        let mut previous = -f32::MAX;
        let steps = 100;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let value = ease_out_cubic(t);
            assert!(value >= previous, "curva descendo em t={t}");
            previous = value;
        }
        assert_eq!(ease_out_cubic(-1.0), 0.0, "satura abaixo de 0");
        assert_eq!(ease_out_cubic(2.0), 1.0, "satura acima de 1");
        assert!((0.0..=1.0).contains(&ease_out_cubic(0.25)));
    }

    /// Tween interpola do início ao fim e termina.
    #[test]
    fn tween_moves_from_to_and_finishes() {
        let tween = Tween::now(0.0, 100.0, 240);
        assert!(
            (tween.value(Instant::now()) - 0.0).abs() < 1e-3,
            "começa em from"
        );
        assert!(!tween.finished(Instant::now()));
        let later = Instant::now() + duration(240) + Duration::from_millis(1);
        let end = tween.value(later);
        assert!((end - 100.0).abs() < 1e-3, "termina em to, foi {end}");
        assert!(tween.finished(later));
    }

    /// `lerp` e `pulse` ficam dentro de 0..=1.
    #[test]
    fn lerp_and_pulse_stay_in_unit_range() {
        assert!((Tween::lerp(10.0, 20.0, 0.5) - 15.0).abs() < 1e-6);
        assert_eq!(Tween::lerp(10.0, 20.0, 2.0), 20.0);
        for phase in [0.0_f64, 0.25, 0.5, 0.75] {
            let value = pulse(1.234, 1.5, phase);
            assert!((0.0..=1.0).contains(&value));
        }
    }
}
