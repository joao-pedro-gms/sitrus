//! Skeleton de carregamento com shimmer (pulse senoidal das durações).

use egui::{Color32, Id, Rect, Sense, Ui, Vec2};
use std::hash::{Hash, Hasher};

use crate::anim;
use crate::theme::Theme;
use crate::tokens::{self, spacing};

/// Desenha `count` linhas de skeleton (altura de list_row) com shimmer.
pub fn skeleton_rows(ui: &mut Ui, theme: &Theme, id: Id, count: usize) {
    for i in 0..count {
        let id = id.with(("skeleton_row", i));
        skeleton_row(ui, theme, id);
        ui.add_space(spacing::X8);
    }
}

/// Uma linha de skeleton: avatar + duas barras de texto com shimmer.
pub fn skeleton_row(ui: &mut Ui, theme: &Theme, id: Id) {
    let height = tokens::sizes::LIST_ROW;
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let time = ui.ctx().time();
    let phase = pseudo_phase(id);

    let radius = crate::theme::corner_radius(tokens::radius::SM);
    let base = theme.bg_elevated;
    let highlight = crate::theme::tint(base, theme.on_accent, 0.10);

    // Avatar circular.
    let avatar = Rect::from_center_size(
        egui::Pos2::new(row.left() + spacing::X12 + 16.0, row.center().y),
        Vec2::splat(32.0),
    );
    paint_shimmer_rect(ui, base, highlight, avatar, time, radius, phase);

    // Duas barras de texto.
    for (bar_index, (bar_w, dy)) in [(row.width() * 0.45, -11.0), (row.width() * 0.28, 11.0)]
        .into_iter()
        .enumerate()
    {
        let bar = Rect::from_min_size(
            egui::Pos2::new(avatar.right() + spacing::X12, row.center().y + dy - 5.0),
            Vec2::new(bar_w, 10.0),
        );
        paint_shimmer_rect(
            ui,
            base,
            highlight,
            bar,
            time,
            radius,
            phase + 0.15 * bar_index as f64,
        );
    }
}

/// Pinta um retângulo com "brilho" deslizando (shimmer).
fn paint_shimmer_rect(
    ui: &Ui,
    base: Color32,
    highlight: Color32,
    rect: Rect,
    time_secs: f64,
    radius: egui::CornerRadius,
    phase: f64,
) {
    let painter = ui.painter();
    painter.rect_filled(rect, radius, base);
    // Banda de brilho que atravessa o retângulo a cada ~2 s.
    let sweep = ((time_secs / 2.0 + phase).rem_euclid(1.0) * 2.0 - 0.5).clamp(0.0, 1.0) as f32;
    let band_w = rect.width().max(rect.height() * 2.0);
    let band_x = rect.left() + sweep * (rect.width() + band_w) - band_w;
    let band = Rect::from_min_size(
        egui::Pos2::new(band_x, rect.top()),
        Vec2::new(band_w, rect.height()),
    );
    let pulse = anim::pulse(time_secs, 2.0, phase);
    let color = Color32::from_rgba_unmultiplied(
        highlight.r(),
        highlight.g(),
        highlight.b(),
        (34.0 + 24.0 * pulse) as u8,
    );
    // Limita o brilho ao retângulo pai.
    let clipped = painter.with_clip_rect(rect);
    clipped.rect_filled(band, radius, color);
}

/// Fase determinística por id (0..1) para não sincronizar linhas.
fn pseudo_phase(id: Id) -> f64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    (hasher.finish() % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shimmer varre em janela 0..1 e fase determinística por id.
    #[test]
    fn sweep_stays_in_window() {
        for t in [0.0_f64, 0.3, 1.2, 3.7] {
            let sweep = ((t / 2.0 + 0.25).rem_euclid(1.0) * 2.0 - 0.5).clamp(0.0, 1.0);
            assert!((0.0..=1.0).contains(&sweep));
        }
    }

    /// Pulo de fase igual para o mesmo id (Hash de Id determinístico na sessão).
    #[test]
    fn phase_is_deterministic_per_id() {
        let a = Id::unique("row-a");
        assert_eq!(pseudo_phase(a), pseudo_phase(a));
        assert_ne!(pseudo_phase(a), pseudo_phase(Id::unique("row-b")));
    }
}
