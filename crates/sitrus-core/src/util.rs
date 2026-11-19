//! Utilitários de domínio sem dependência de UI.

use std::time::Duration;

/// Formata uma duração como `m:ss` ou, a partir de uma hora, `h:mm:ss`.
#[must_use]
pub fn fmt_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    if total_minutes >= 60 {
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{total_minutes}:{seconds:02}")
    }
}
