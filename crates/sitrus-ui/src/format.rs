//! Formatação de duração para a UI (faixas de lista, seek, fila).
//!
//! Formato Apple Music: `m:ss` até 1 hora, `h:mm:ss` a partir de 1 hora.

/// Formata segundos totais como duração de faixa.
///
/// `59 → "0:59"`, `83 → "1:23"`, `3600 → "1:00:00"`, `3_675 → "1:01:15"`.
#[must_use]
pub fn format_duration(total_seconds: u32) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Formata "posição atual / duração total" para a seek bar.
#[must_use]
pub fn format_position(position_secs: u32, duration_secs: u32) -> String {
    format!(
        "{} / {}",
        format_duration(position_secs),
        format_duration(duration_secs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minutos e segundos, sem zero à esquerda no minuto.
    #[test]
    fn minutes_seconds_format() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(5), "0:05");
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(60), "1:00");
        assert_eq!(format_duration(83), "1:23");
        assert_eq!(format_duration(599), "9:59");
        assert_eq!(format_duration(600), "10:00");
    }

    /// Horas entram com o formato `h:mm:ss`.
    #[test]
    fn hours_format_when_one_full_hour_or_more() {
        assert_eq!(format_duration(3599), "59:59");
        assert_eq!(format_duration(3600), "1:00:00");
        assert_eq!(format_duration(3_675), "1:01:15");
        assert_eq!(format_duration(43_381), "12:03:01");
    }

    /// A barra seek compõe os dois relógios.
    #[test]
    fn position_composes_both_clocks() {
        assert_eq!(format_position(42, 210), "0:42 / 3:30");
        assert_eq!(format_position(3_671, 4_000), "1:01:11 / 1:06:40");
    }
}
