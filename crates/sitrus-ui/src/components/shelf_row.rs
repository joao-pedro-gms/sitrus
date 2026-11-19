//! Prateleira horizontal (ShelfRow): rolagem com snap e setas de navegação.

use std::time::Instant;

use egui::{Align, Id, Layout, Response, Sense, Ui, UiBuilder, Vec2};

use crate::anim::{self, Tween};
use crate::components::{album_card, button, section_header};
use crate::icons::Icon;
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing};

/// Passo de snap: largura do card + espaçamento.
pub const SNAP_STEP: f32 = album_card::CARD_WIDTH + spacing::X16;

/// Resultado da prateleira.
#[derive(Debug)]
pub struct ShelfOutput {
    /// Resposta de "Ver todos".
    pub see_all: Response,
    /// Largura útil da área de rolagem.
    pub viewport_width: f32,
}

/// Prateleira horizontal de cards com título, "Ver todos", setas e snap.
///
/// `render_item` desenha o card de índice `i` dentro da célula.
pub fn shelf_row<R>(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    title: &str,
    item_count: usize,
    render_item: impl FnMut(&mut Ui, usize) -> R,
) -> ShelfOutput {
    let see_all =
        section_header::section_header(ui, theme, id.with("header"), title, Some("Ver todos"));

    // Estado de rolagem animada (px de offset alvo + tween em curso).
    let target_id = id.with("scroll_target");
    let tween_id = id.with("scroll_tween");
    let mut target: f32 = ui
        .ctx()
        .data_mut(|d| d.get_temp::<f32>(target_id))
        .unwrap_or(0.0);
    let mut tween: Option<Tween> = ui.ctx().data_mut(|d| d.get_temp::<Tween>(tween_id));

    let viewport = ui.available_width();
    let output = {
        let mut builder = egui::ScrollArea::horizontal()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, true]);
        if let Some(t) = &tween {
            builder = builder.horizontal_scroll_offset(t.value(Instant::now()));
        }
        builder.show(ui, |ui| {
            ui.horizontal(|row| {
                let mut render = render_item;
                for i in 0..item_count {
                    let cell_size = Vec2::new(album_card::CARD_WIDTH, album_card::CARD_HEIGHT);
                    let (cell, _) = row.allocate_exact_size(cell_size, Sense::hover());
                    let mut cell_ui = row.new_child(
                        UiBuilder::new()
                            .id_salt(id.with(("cell", i)))
                            .max_rect(cell)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    render(&mut cell_ui, i);
                }
            });
        })
    };
    let current = output.state.clamped_offset().x;
    let max_offset = (SNAP_STEP * item_count.max(1) as f32 - viewport).max(0.0);
    target = target.clamp(0.0, max_offset);

    // Snap: ponteiro solto e sem inércia ⇒ almeja o múltiplo próximo (180 ms).
    let pointer_down = ui.input(|i| i.pointer.any_down());
    let inertia = output.state.velocity().x.abs() > 50.0;
    if !pointer_down && !inertia && tween.is_none() {
        let snapped = ((current / SNAP_STEP).round() * SNAP_STEP).clamp(0.0, max_offset);
        if (snapped - current).abs() > 0.5 {
            tween = Some(Tween::now(current, snapped, anim::STATE_MS));
            ui.ctx().data_mut(|d| {
                d.insert_temp(tween_id, tween.expect("tween recém-criado"));
                d.insert_temp(target_id, snapped);
            });
        }
    }
    if let Some(t) = &tween
        && t.finished(Instant::now())
    {
        ui.ctx().data_mut(|d| d.remove::<Tween>(tween_id));
    }

    // Setas superpostas nas bordas da área de rolagem.
    let viewport_rect = output.inner_rect;
    let left_rect = egui::Rect::from_center_size(
        egui::Pos2::new(viewport_rect.left() + 20.0, viewport_rect.center().y),
        Vec2::splat(32.0),
    );
    let right_rect = egui::Rect::from_center_size(
        egui::Pos2::new(viewport_rect.right() - 20.0, viewport_rect.center().y),
        Vec2::splat(32.0),
    );
    let left_arrow = arrow_button(
        ui,
        left_rect,
        id.with("arrow_left"),
        theme,
        can_go(current, 0.0, false),
        Icon::ChevronLeft,
    );
    let right_arrow = arrow_button(
        ui,
        right_rect,
        id.with("arrow_right"),
        theme,
        can_go(current, max_offset, true),
        Icon::ChevronRight,
    );

    let next_target = if left_arrow.clicked() {
        Some((target - SNAP_STEP).max(0.0))
    } else if right_arrow.clicked() {
        Some((target + SNAP_STEP).min(max_offset))
    } else {
        None
    };
    if let Some(next) = next_target {
        let tween_now = Tween::now(current, next, anim::TRANSITION_MS);
        ui.ctx().data_mut(|d| {
            d.insert_temp(tween_id, tween_now);
            d.insert_temp(target_id, next);
        });
    }

    ui.add_space(spacing::X8);
    ShelfOutput {
        see_all,
        viewport_width: viewport,
    }
}

fn can_go(current: f32, bound: f32, forward: bool) -> bool {
    if forward {
        current < bound - 0.5
    } else {
        current > bound + 0.5
    }
}

/// Seta circular superposta; estados derivam da mesma máquina dos botões.
fn arrow_button(
    ui: &mut Ui,
    rect: egui::Rect,
    id: Id,
    theme: &Theme,
    enabled: bool,
    icon: Icon,
) -> Response {
    let response = ui.interact(rect, id, Sense::CLICK.union(Sense::FOCUSABLE));
    let state = button::derive_state(
        enabled,
        false,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );
    let color = match state {
        button::ButtonState::Normal => theme.label_secondary,
        button::ButtonState::Hovered | button::ButtonState::Pressed => theme.label_primary,
        button::ButtonState::Focused => theme.label_primary,
        button::ButtonState::Disabled => theme.label_tertiary,
        button::ButtonState::Selected => theme.accent,
    };
    if matches!(
        state,
        button::ButtonState::Hovered | button::ButtonState::Pressed
    ) {
        let bg = if state == button::ButtonState::Pressed {
            theme.bg_selected
        } else {
            theme.bg_hover
        };
        ui.painter_at(rect)
            .rect_filled(rect, theme::corner_radius(tokens::radius::PILL), bg);
    }
    if state == button::ButtonState::Focused {
        button::draw_focus_ring(ui, theme, rect, tokens::radius::PILL);
    }
    crate::icons::draw(ui.painter(), icon, rect.center(), 18.0, color);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snap usa passo = card 180 + espaçamento.
    #[test]
    fn snap_step_is_card_plus_gap() {
        assert_eq!(SNAP_STEP, 180.0 + spacing::X16);
    }

    /// `can_go` limita a navegação às bordas.
    #[test]
    fn arrows_disable_at_boundaries() {
        assert!(!can_go(0.0, 0.0, false), "não dá para voltar no início");
        assert!(!can_go(100.0, 100.0, true), "não dá para avançar no fim");
        assert!(can_go(200.0, 400.0, true));
        assert!(can_go(200.0, 0.0, false));
    }
}
