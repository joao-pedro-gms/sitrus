//! Campo de busca: pill com lupa, placeholder e botão de limpar.

use egui::{Align2, Id, Rect, Response, Sense, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Altura do campo.
pub const SEARCH_HEIGHT: f32 = 36.0;

/// Desenha o campo; retorna a resposta do TextEdit (focus/click).
pub fn search_field(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    text: &mut String,
    enabled: bool,
) -> Response {
    let height = SEARCH_HEIGHT;
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let response = ui.interact(area, id, Sense::CLICK.union(Sense::FOCUSABLE));
    let focused = response.has_focus();

    let radius = theme::corner_radius(tokens::radius::PILL);
    let fill = match (enabled, focused) {
        (false, _) => theme.bg_rail,
        (_, true) => theme.bg_hover,
        (_, false) => theme.bg_elevated,
    };
    ui.painter().rect_filled(area, radius, fill);
    if focused {
        crate::components::button::draw_focus_ring(ui, theme, area, tokens::radius::PILL);
    } else {
        ui.painter().rect_stroke(
            area,
            radius,
            egui::Stroke::new(1.0, theme.separator),
            egui::StrokeKind::Inside,
        );
    }

    let icon_color = if enabled {
        theme.label_secondary
    } else {
        theme.label_tertiary
    };
    icons::draw(
        ui.painter(),
        Icon::Search,
        egui::Pos2::new(area.left() + spacing::X16, area.center().y),
        16.0,
        icon_color,
    );

    // TextEdit invisível dentro da pílula.
    let edit_x = area.left() + spacing::X32;
    let clear_w = if text.is_empty() || !enabled {
        0.0
    } else {
        spacing::X24
    };
    let edit_rect = Rect::from_min_size(
        egui::Pos2::new(edit_x, area.top() + 4.0),
        Vec2::new(
            area.width() - (edit_x - area.left()) - clear_w - spacing::X12 - spacing::X8,
            height - 8.0,
        ),
    );
    let mut edit_child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id.with("edit"))
            .max_rect(edit_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let painter = ui.painter();
    let _edit_response = if enabled {
        egui::TextEdit::singleline(text)
            .id(id.with("edit_widget"))
            .desired_width(edit_rect.width())
            .frame(egui::Frame::NONE)
            .hint_text("Buscar músicas, artistas, álbuns…")
            .font(egui::FontSelection::FontId(typography::font_id(
                type_scale::TypeToken::Body,
            )))
            .text_color(theme.label_primary)
            .show(&mut edit_child)
            .response
            .response
    } else {
        painter.text(
            edit_rect.left_center(),
            Align2::LEFT_CENTER,
            "Buscar (indisponível)",
            typography::font_id(type_scale::TypeToken::Body),
            theme.label_tertiary,
        );
        edit_child.allocate_exact_size(Vec2::ZERO, Sense::hover()).1
    };

    // Botão de limpar.
    if !text.is_empty() && enabled {
        let clear_rect = Rect::from_center_size(
            egui::Pos2::new(area.right() - spacing::X16, area.center().y),
            Vec2::splat(tokens::sizes::MIN_CLICK_TARGET),
        );
        let clear = painter.circle_filled(clear_rect.center(), 9.0, theme.bg_selected);
        let _ = clear;
        icons::draw(
            painter,
            Icon::Close,
            clear_rect.center(),
            12.0,
            theme.label_secondary,
        );
        let clear_response = ui.interact(clear_rect, id.with("clear"), Sense::CLICK);
        if clear_response.clicked() {
            text.clear();
        }
    }

    response.on_hover_text(if enabled {
        "Pressiona / para focar na busca (atalho da janela)"
    } else {
        "Busca indisponível agora"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O campo respeita altura token e alvo mínimo do botão de limpar.
    #[test]
    fn field_measurements() {
        assert_eq!(SEARCH_HEIGHT, 36.0);
        const { assert!(tokens::sizes::MIN_CLICK_TARGET <= SEARCH_HEIGHT) }
    }
}
