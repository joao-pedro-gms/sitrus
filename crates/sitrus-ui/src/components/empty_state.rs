//! Estado vazio: ícone em círculo, título, mensagem e ação opcional.

use egui::{Align2, Id, Response, Ui, UiBuilder, Vec2};

use crate::components::button::{self, ButtonContent, ButtonVariant};
use crate::icons::{self, Icon};
use crate::theme::Theme;
use crate::tokens::{spacing, type_scale};
use crate::typography;

/// Desenha o empty state no espaço disponível; devolve a resposta da ação.
pub fn empty_state(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    icon: Icon,
    title: &str,
    message: &str,
    action: Option<&str>,
) -> Option<Response> {
    let available = ui.available_size();
    let (area, _) = ui.allocate_exact_size(
        Vec2::new(available.x, 280.0_f32.min(available.y)),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let cx = area.center().x;
    let mut y = area.center().y - 80.0;

    // Ícone em círculo elevado.
    let circle = egui::Rect::from_center_size(egui::Pos2::new(cx, y + 32.0), Vec2::splat(64.0));
    painter.rect_filled(
        circle,
        crate::theme::corner_radius(crate::tokens::radius::PILL),
        theme.bg_elevated,
    );
    icons::draw(painter, icon, circle.center(), 28.0, theme.accent);
    y = circle.bottom() + spacing::X16;

    painter.text(
        egui::Pos2::new(cx, y),
        Align2::LEFT_TOP,
        title,
        typography::font_id(type_scale::TypeToken::Title2),
        theme.label_primary,
    );
    // centraliza texto: usa anchor CENTER
    painter.text(
        egui::Pos2::new(
            cx,
            y + type_scale::TypeToken::Title2.spec().line_height + spacing::X4,
        ),
        Align2::CENTER_TOP,
        message,
        typography::font_id(type_scale::TypeToken::Subheadline),
        theme.label_secondary,
    );
    let action_y = y
        + type_scale::TypeToken::Title2.spec().line_height
        + spacing::X8
        + type_scale::TypeToken::Subheadline.spec().line_height
        + spacing::X16;

    action.map(|label| {
        let button_rect =
            egui::Rect::from_min_size(egui::Pos2::new(cx - 70.0, action_y), Vec2::new(140.0, 40.0));
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt(id.with("empty_action"))
                .max_rect(button_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        button::button(
            &mut child,
            theme,
            id,
            ButtonVariant::Secondary,
            ButtonContent::label(label),
            true,
            false,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O bloco vertical cabe em 280 px (padrão do empty state).
    #[test]
    fn empty_state_stays_in_block_height() {
        let icon = 64.0 + spacing::X16;
        let title = type_scale::TypeToken::Title2.spec().line_height + spacing::X8;
        let message = type_scale::TypeToken::Subheadline.spec().line_height + spacing::X16;
        let action = 40.0 + spacing::X16;
        assert!(icon + title + message + action <= 280.0);
    }
}
