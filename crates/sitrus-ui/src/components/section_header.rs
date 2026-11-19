//! Cabeçalho de seção: título Title2 + "Ver tudo" no lado direito.

use egui::{Align2, Id, Response, Sense, Ui, UiBuilder, Vec2};

use crate::components::button::{self, ButtonVariant};
use crate::theme::Theme;
use crate::tokens::type_scale;
use crate::typography;

/// Altura da linha do cabeçalho.
pub const SECTION_HEADER_HEIGHT: f32 = 34.0;

/// Desenha o cabeçalho; retorna a resposta de "Ver tudo" (se exibido).
pub fn section_header(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    title: &str,
    see_all: Option<&str>,
) -> Response {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, SECTION_HEADER_HEIGHT), Sense::hover());
    let painter = ui.painter();
    painter.text(
        row.left_center(),
        Align2::LEFT_CENTER,
        title,
        typography::font_id(type_scale::TypeToken::Title2),
        theme.label_primary,
    );

    match see_all {
        Some(label) => {
            let button_area = egui::Rect::from_min_size(
                egui::Pos2::new(row.right() - 90.0, row.top()),
                Vec2::new(90.0, row.height()),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id_salt(id.with("see_all"))
                    .max_rect(button_area)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            button::button(
                &mut child,
                theme,
                id,
                ButtonVariant::Plain,
                button::ButtonContent::label(label),
                true,
                false,
            )
        }
        None => {
            let (_, filler) = ui.allocate_exact_size(Vec2::ZERO, Sense::hover());
            filler
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::TypeToken;

    /// O cabeçalho usa Title2 (22 px) e cabe em 34 px de linha.
    #[test]
    fn header_uses_title2_with_room_for_line_height() {
        let spec = TypeToken::Title2.spec();
        assert_eq!(spec.size, 22.0);
        assert!(SECTION_HEADER_HEIGHT >= spec.line_height);
    }
}
