//! Toast: notificação leve com ícone, mensagem e auto-dismiss cronometrado.

use egui::{Align2, Color32, Id, Rect, Response, Sense, Ui, Vec2};

use crate::components::error_banner::BannerKind;
use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Duração padrão de exibição (3 s).
pub const DEFAULT_TOAST_DURATION_SECS: f64 = 3.0;

/// Ativa o toast para o próximo(s) frame(s) até esgotar a duração.
pub fn trigger_toast(ui: &Ui, id: Id, duration_secs: f64) {
    let until = ui.ctx().time() + duration_secs.max(0.05);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("until"), until));
}

/// Desenha o toast se ainda visível; devolve `Some(Response)` do fechar.
pub fn toast(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    kind: BannerKind,
    message: &str,
) -> Option<Response> {
    let until: f64 = ui.ctx().data(|d| d.get_temp(id.with("until")))?;
    let now = ui.ctx().time();
    if now >= until {
        ui.ctx().data_mut(|d| d.remove::<f64>(id.with("until")));
        return None;
    }

    let color = kind.color(theme);
    let (area, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(240.0), 44.0),
        Sense::hover(),
    );
    let painter = ui.painter();
    // Card escuro elevado com sombra leve.
    painter.rect_filled(
        area,
        theme::corner_radius(tokens::radius::LG),
        theme::tint(theme.bg_elevated, Color32::BLACK, 0.25),
    );
    painter.rect_stroke(
        area,
        theme::corner_radius(tokens::radius::LG),
        egui::Stroke::new(1.5, theme::tint(color, theme.bg_content, 0.3)),
        egui::StrokeKind::Inside,
    );
    icons::draw(
        painter,
        kind.icon(),
        egui::Pos2::new(area.left() + spacing::X20, area.center().y),
        20.0,
        color,
    );
    painter.text(
        egui::Pos2::new(area.left() + spacing::X40, area.center().y),
        Align2::LEFT_CENTER,
        message,
        typography::font_id(type_scale::TypeToken::Body),
        theme.label_primary,
    );
    // Barra de tempo restante (linha fina embaixo).
    let remaining = ((until - now) / DEFAULT_TOAST_DURATION_SECS).clamp(0.0, 1.0);
    let progress = Rect::from_min_size(
        area.left_bottom(),
        Vec2::new(area.width() * remaining as f32, 2.0),
    );
    painter.rect_filled(progress, 1.0, color);

    let close_rect = egui::Rect::from_center_size(
        egui::Pos2::new(area.right() - spacing::X20, area.center().y),
        Vec2::splat(tokens::sizes::MIN_CLICK_TARGET),
    );
    icons::draw(
        painter,
        Icon::Close,
        close_rect.center(),
        14.0,
        theme.label_secondary,
    );
    let response = ui.interact(close_rect, id.with("toast_close"), Sense::CLICK);
    if response.clicked() {
        ui.ctx().data_mut(|d| d.remove::<f64>(id.with("until")));
    }
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Percentual restante clampa entre 0 e 1.
    #[test]
    fn toast_progress_clamps() {
        let remaining =
            |until: f64, now: f64| ((until - now) / DEFAULT_TOAST_DURATION_SECS).clamp(0.0, 1.0);
        assert_eq!(remaining(6.0, 3.0), 1.0);
        assert_eq!(remaining(3.0, 3.5), 0.0);
        assert!((remaining(4.0, 3.0) - (1.0 / 3.0)).abs() < 1e-3);
    }
}
