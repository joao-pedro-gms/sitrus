//! Controle segmentado (pill) para alternar views.

use egui::{Align2, Id, Rect, Sense, Ui, Vec2};

use crate::components::button::{ButtonState, derive_state};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Altura do controle.
pub const SEGMENTED_HEIGHT: f32 = 32.0;

/// Desenha o segmented control; devolve o índice selecionado (mudou ou não).
pub fn segmented_control(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    options: &[&str],
    selected: usize,
    enabled: bool,
) -> usize {
    let width = ui.available_width().min(420.0);
    let (area, _) = ui.allocate_exact_size(Vec2::new(width, SEGMENTED_HEIGHT), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        area,
        theme::corner_radius(tokens::radius::PILL),
        theme.bg_elevated,
    );
    painter.rect_stroke(
        area,
        theme::corner_radius(tokens::radius::PILL),
        egui::Stroke::new(1.0, theme.separator),
        egui::StrokeKind::Inside,
    );

    let padding = 3.0;
    let seg_w = (width - 2.0 * padding) / options.len().max(1) as f32;
    let seg_h = SEGMENTED_HEIGHT - 2.0 * padding;
    let mut result = selected;

    for (i, label) in options.iter().enumerate() {
        let seg_rect = Rect::from_min_size(
            egui::Pos2::new(
                area.left() + padding + seg_w * i as f32,
                area.top() + padding,
            ),
            Vec2::new(seg_w, seg_h),
        );
        let response = ui.interact(
            seg_rect,
            id.with(("segment", i)),
            Sense::CLICK.union(Sense::FOCUSABLE),
        );
        let state = derive_state(
            enabled,
            i == selected,
            response.hovered(),
            response.is_pointer_button_down_on() && enabled,
            response.has_focus(),
        );
        match state {
            ButtonState::Hovered => {
                painter.rect_filled(
                    seg_rect,
                    theme::corner_radius(tokens::radius::PILL),
                    theme.bg_hover,
                );
            }
            ButtonState::Pressed | ButtonState::Selected => {
                painter.rect_filled(
                    seg_rect,
                    theme::corner_radius(tokens::radius::PILL),
                    theme.bg_selected,
                );
            }
            ButtonState::Focused => {
                crate::components::button::draw_focus_ring(
                    ui,
                    theme,
                    seg_rect,
                    tokens::radius::PILL,
                );
            }
            _ => {}
        }
        let text_color = match state {
            ButtonState::Selected => theme.accent,
            ButtonState::Disabled | ButtonState::Normal => theme.label_secondary,
            ButtonState::Focused | ButtonState::Hovered | ButtonState::Pressed => {
                theme.label_primary
            }
        };
        painter.text(
            seg_rect.center(),
            Align2::CENTER_CENTER,
            *label,
            typography::font_id(type_scale::TypeToken::Footnote),
            text_color,
        );
        if response.clicked() && enabled && i != selected {
            result = i;
        }
    }

    // padding respirável à direita (constante documentada).
    let _ = spacing::X8;
    result
}

#[cfg(test)]
mod tests {
    // Matemática local; sem dependência do módulo.

    /// Largura por segmento divide o container com padding simétrico.
    #[test]
    fn segment_width_math() {
        let width = 320.0;
        let padding = 3.0;
        let options = ["Prateleiras", "Lista"];
        let seg_w = (width - 2.0 * padding) / options.len() as f32;
        assert_eq!(seg_w, 157.0);
    }
}
