//! Abas (tabs): rótulos horizontais com sublinhado do acento na aba ativa —
//! usadas na Busca (Artistas/Álbuns/Faixas) e no artista (Álbuns/Músicas).
//!
//! A matemática de seleção é pura ([`advance_selection`]) para ser testável
//! sem contexto gráfico; o wrapper de UI só conecta resposta ↔ troca de aba.

use egui::{Align2, Id, Sense, Ui, Vec2};

use crate::components::button::{self, ButtonState};
use crate::theme::{self, Theme};
use crate::tokens::{self, type_scale};
use crate::typography;

/// Altura da faixa de abas.
pub const TABS_HEIGHT: f32 = 32.0;
/// Espessura do sublinhado da aba ativa.
pub const UNDERLINE_HEIGHT: f32 = 2.0;
/// Espaço horizontal entre abas.
const TAB_GAP: f32 = 24.0;
/// Respiro interno por aba (além da largura do texto).
const TAB_INNER_PADDING: f32 = 8.0;

/// Determina a nova aba a partir de um clique, ignorando cliques fora da
/// faixa de abas e repetições da aba corrente. `None` quando não muda.
#[must_use]
pub fn advance_selection(current: usize, clicked: Option<usize>, count: usize) -> Option<usize> {
    match clicked {
        Some(i) if i < count && i != current => Some(i),
        _ => None,
    }
}

/// Desenha a faixa de abas; devolve a nova aba quando o usuário trocou.
pub fn tabs(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    labels: &[&str],
    selected: usize,
    enabled: bool,
) -> Option<usize> {
    let font = typography::font_id(type_scale::TypeToken::Subheadline);
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), theme.label_primary)
                .rect
                .width()
        })
        .collect();
    let total = widths
        .iter()
        .map(|w| w + 2.0 * TAB_INNER_PADDING)
        .sum::<f32>()
        + widths.len().saturating_sub(1) as f32 * TAB_GAP;
    let (area, _) = ui.allocate_exact_size(Vec2::new(total, TABS_HEIGHT), Sense::hover());
    let painter = ui.painter();

    let mut clicked: Option<usize> = None;
    let mut cursor_x = area.left();
    for (i, label) in labels.iter().enumerate() {
        let label_width = widths.get(i).copied().unwrap_or(0.0);
        let tab_rect = egui::Rect::from_min_size(
            egui::Pos2::new(cursor_x, area.top() + 4.0),
            Vec2::new(label_width + 2.0 * TAB_INNER_PADDING, TABS_HEIGHT - 8.0),
        );
        let response = ui.interact(tab_rect, id.with(("tab", i)), Sense::CLICK);
        let state = button::derive_state(
            enabled,
            i == selected,
            response.hovered(),
            response.is_pointer_button_down_on() && enabled,
            response.has_focus(),
        );
        let text_color = match state {
            ButtonState::Selected | ButtonState::Hovered | ButtonState::Pressed => {
                theme.label_primary
            }
            ButtonState::Disabled => theme.label_tertiary,
            ButtonState::Normal | ButtonState::Focused => theme.label_secondary,
        };
        painter.text(
            tab_rect.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            text_color,
        );
        // Sublinhado do acento sob a aba ativa.
        if i == selected {
            let baseline = egui::Rect::from_min_size(
                egui::Pos2::new(
                    tab_rect.left() + TAB_INNER_PADDING,
                    area.bottom() - UNDERLINE_HEIGHT,
                ),
                Vec2::new(
                    (tab_rect.width() - 2.0 * TAB_INNER_PADDING).max(0.0),
                    UNDERLINE_HEIGHT,
                ),
            );
            painter.rect_filled(
                baseline,
                theme::corner_radius(tokens::radius::SM),
                theme.accent,
            );
        }
        if response.clicked() && enabled {
            clicked = Some(i);
        }
        cursor_x = tab_rect.right() + TAB_GAP;
    }
    advance_selection(selected, clicked, labels.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clique em aba válida e diferente troca a seleção.
    #[test]
    fn click_changes_to_clicked_tab() {
        assert_eq!(advance_selection(0, Some(2), 3), Some(2));
        assert_eq!(advance_selection(2, Some(0), 3), Some(0));
    }

    /// Clique na aba corrente não sinaliza troca.
    #[test]
    fn click_on_current_tab_is_noop() {
        assert_eq!(advance_selection(1, Some(1), 3), None);
    }

    /// Clique fora da faixa de abas é ignorado.
    #[test]
    fn click_out_of_range_is_ignored() {
        assert_eq!(advance_selection(0, Some(3), 3), None);
        assert_eq!(advance_selection(0, Some(99), 3), None);
        assert_eq!(advance_selection(0, None, 3), None);
    }

    /// Faixa de abas com uma única aba nunca troca.
    #[test]
    fn single_tab_never_changes() {
        assert_eq!(advance_selection(0, Some(0), 1), None);
    }
}
