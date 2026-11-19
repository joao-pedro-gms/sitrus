//! Rail de navegação de 72 px (direção B): só ícone + tooltip.
//!
//! Zonas do componente cobertas: rail inteiro, `rail_item` individual e
//! tooltip; estados normal/hover/pressed/focus/disabled/selecionado.

use egui::{Align, Color32, Id, Layout, Response, Sense, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing};

/// Largura do rail (design: 72 px).
pub const RAIL_WIDTH: f32 = 72.0;
/// Tamanho do glifo do rail.
pub const RAIL_ICON_SIZE: f32 = 22.0;

/// Um item do rail.
#[derive(Clone, Copy, Debug)]
pub struct RailItem<'a> {
    /// Ícone original (desenho próprio).
    pub icon: Icon,
    /// Tooltip/label (pt-BR na UI do app).
    pub label: &'a str,
    /// Item indisponível (pintado apagado, ignora hover).
    pub enabled: bool,
}

impl<'a> RailItem<'a> {
    /// Item habilitado.
    #[must_use]
    pub const fn new(icon: Icon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            enabled: true,
        }
    }
}

/// Desenha o rail completo; devolve a resposta de cada item, na ordem.
///
/// `selected: Option<usize>` marca o item ativo; `id_base` garante ids
/// únicos quando o rail aparece em duas versões (claro/escuro).
pub fn navigation_rail(
    ui: &mut Ui,
    theme: &Theme,
    id_base: Id,
    items: &[RailItem<'_>],
    selected: Option<usize>,
) -> Vec<Response> {
    let mut responses = Vec::with_capacity(items.len());
    ui.allocate_ui(Vec2::new(RAIL_WIDTH, ui.available_height()), |rail_area| {
        let (rail_rect, _) = rail_area.allocate_exact_size(
            Vec2::new(RAIL_WIDTH, rail_area.available_height()),
            Sense::hover(),
        );
        rail_area
            .painter()
            .rect_filled(rail_rect, 0.0, theme.bg_rail);

        rail_area.vertical(|column| {
            column.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(spacing::X24);
                for (index, item) in items.iter().enumerate() {
                    let id = id_base.with(("sitrus_rail", index));
                    responses.push(rail_item(ui, theme, id, *item, selected == Some(index)));
                    ui.add_space(spacing::X8);
                }
            });
        });
    });
    responses
}

/// Um item do rail: caixa 40×40 radio `md`, ícone central, tooltip no hover.
pub fn rail_item(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    item: RailItem<'_>,
    selected: bool,
) -> Response {
    let size = Vec2::splat(tokens::sizes::RAIL_ITEM);
    let (rect, _) = ui.allocate_exact_size(size, Sense::CLICK.union(Sense::FOCUSABLE));
    let response = ui.interact(rect, id, Sense::CLICK.union(Sense::FOCUSABLE));
    let enabled = item.enabled;
    let state = crate::components::button::derive_state(
        enabled,
        selected,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );

    let (bg, icon_color) = match state {
        crate::components::button::ButtonState::Normal => (None, theme.label_secondary),
        crate::components::button::ButtonState::Hovered => {
            (Some(theme.bg_hover), theme.label_primary)
        }
        crate::components::button::ButtonState::Pressed => {
            (Some(theme.bg_selected), theme.label_primary)
        }
        crate::components::button::ButtonState::Focused => (None, theme.label_primary),
        crate::components::button::ButtonState::Disabled => (None, theme.label_tertiary),
        crate::components::button::ButtonState::Selected => (Some(theme.bg_selected), theme.accent),
    };
    let painter = ui.painter();
    if let Some(bg) = bg {
        painter.rect_filled(rect, theme::corner_radius(tokens::radius::MD), bg);
    }
    if state == crate::components::button::ButtonState::Focused {
        crate::components::button::draw_focus_ring(ui, theme, rect, tokens::radius::MD);
    }
    icons::draw(
        painter,
        item.icon,
        rect.center(),
        RAIL_ICON_SIZE,
        icon_color,
    );
    if enabled {
        response.on_hover_text(item.label.to_owned())
    } else {
        response.on_disabled_hover_text(item.label)
    }
}

/// Cor do ícone do rail em função do estado (exposta para testes/temas).
#[must_use]
pub fn rail_icon_color(theme: &Theme, selected: bool, disabled: bool) -> Color32 {
    if disabled {
        theme.label_tertiary
    } else if selected {
        theme.accent
    } else {
        theme.label_secondary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::button::{ButtonState, derive_state};

    /// Estados do rail item seguem a mesma precedência dos botões.
    #[test]
    fn rail_item_states_follow_button_precedence() {
        assert_eq!(
            derive_state(false, true, false, false, false),
            ButtonState::Disabled
        );
        assert_eq!(
            derive_state(true, true, true, false, false),
            ButtonState::Hovered
        );
        assert_eq!(
            derive_state(true, true, false, false, false),
            ButtonState::Selected
        );
        assert_eq!(
            derive_state(true, false, false, false, false),
            ButtonState::Normal
        );
    }

    /// Cor do ícone: acento no selecionado, terciário no desabilitado.
    #[test]
    fn rail_icon_color_semantics() {
        let theme = Theme::dark();
        assert_eq!(rail_icon_color(&theme, true, false), theme.accent);
        assert_eq!(rail_icon_color(&theme, false, true), theme.label_tertiary);
        assert_eq!(rail_icon_color(&theme, false, false), theme.label_secondary);
    }
}
