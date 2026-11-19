//! Modal central com scrim clicável (login, confirmações).

use egui::{Align2, Color32, Id, Order, Rect, Sense, Ui, UiBuilder, Vec2};

use crate::components::button::{self, ButtonContent, ButtonVariant};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Resultado do modal.
#[derive(Debug)]
pub struct ModalOutcome {
    /// Scrim/preencher clicado (deve fechar).
    pub dismissed: bool,
    /// Botão primário clicado.
    pub confirmed: bool,
}

/// Abre um modal centralizado: scrim + cartão com título, corpo e ação.
///
/// Desenhar o modal todo frame enquanto aberto; `confirmed` manda fechar.
pub fn modal(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    title: &str,
    body: impl FnOnce(&mut Ui),
    primary_label: &str,
) -> ModalOutcome {
    let ctx = ui.ctx();
    let screen = ctx.viewport(|v| v.input.content_rect());

    // Scrim: bloqueia e desfaz ao clicar fora.
    let scrim_id = id.with("scrim");
    egui::Area::new(scrim_id.with("area"))
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.painter()
                .rect_filled(screen.expand(300.0), 0.0, Color32::from_black_alpha(140));
            let response = ui.interact(screen, scrim_id, Sense::CLICK);
            let _ = response;
        });
    let scrim_clicked = ctx
        .read_response(scrim_id)
        .map(|r| r.clicked())
        .unwrap_or(false);

    // Cartão central.
    let card_width = 440.0_f32.min(screen.width() - 40.0);
    let card_height = 260.0;
    let dismissed = std::cell::Cell::new(false);
    let mut confirmed = false;
    egui::Area::new(id.with("card_area"))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_clip_rect(screen.expand(300.0));
            let card =
                egui::Rect::from_center_size(screen.center(), Vec2::new(card_width, card_height));
            let painter = ui.painter();
            painter.rect_filled(
                card,
                theme::corner_radius(tokens::radius::LG),
                theme.bg_elevated,
            );
            painter.rect_stroke(
                card,
                theme::corner_radius(tokens::radius::LG),
                egui::Stroke::new(1.0, theme.separator),
                egui::StrokeKind::Inside,
            );
            painter.text(
                egui::Pos2::new(card.left() + spacing::X24, card.top() + spacing::X24),
                Align2::LEFT_TOP,
                title,
                typography::font_id(type_scale::TypeToken::Title1),
                theme.label_primary,
            );

            let mut body_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt(id.with("body"))
                    .max_rect(Rect::from_min_size(
                        egui::Pos2::new(
                            card.left() + spacing::X24,
                            card.top()
                                + spacing::X24
                                + type_scale::TypeToken::Title1.spec().line_height,
                        ),
                        Vec2::new(card.width() - 2.0 * spacing::X24, card_height - 120.0),
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            body(&mut body_ui);

            // Barra de ações.
            let actions = Rect::from_min_size(
                egui::Pos2::new(card.right() - 240.0, card.bottom() - spacing::X16 - 40.0),
                Vec2::new(240.0 - spacing::X16, 40.0),
            );
            let mut actions_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt(id.with("actions"))
                    .max_rect(actions)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            let primary = button::button(
                &mut actions_ui,
                theme,
                id.with("primary"),
                ButtonVariant::Primary,
                ButtonContent::label(primary_label),
                true,
                false,
            );
            if primary.clicked() {
                confirmed = true;
            }
            let cancel = button::button(
                &mut actions_ui,
                theme,
                id.with("cancel"),
                ButtonVariant::Plain,
                ButtonContent::label("Cancelar"),
                true,
                false,
            );
            if cancel.clicked() {
                dismissed.set(true);
            }
        });

    ModalOutcome {
        dismissed: dismissed.get() || scrim_clicked,
        confirmed,
    }
}

/// Fecha o modal (limpa memória de estado, se usada pela galeria).
pub fn close_modal(ui: &Ui, id: Id) {
    ui.ctx().data_mut(|d| d.remove::<bool>(id.with("open")));
}

#[cfg(test)]
mod tests {
    /// Cartão respeita margem mínima da tela.
    #[test]
    fn card_width_never_overflows_small_screens() {
        let card_width = 440.0_f32.min(640.0 - 40.0);
        assert_eq!(card_width, 440.0);
        let tiny = 440.0_f32.min(300.0 - 40.0);
        assert_eq!(tiny, 260.0);
    }
}
