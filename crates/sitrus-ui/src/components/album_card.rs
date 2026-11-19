//! Card de álbum para prateleiras: artwork com cantos + título + subtítulo.

use egui::{Align2, Id, Response, Sense, Ui, Vec2};

use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Card completo de prateleira (design: artwork 180 + textos, altura 224).
pub const CARD_WIDTH: f32 = tokens::sizes::SHELF_ARTWORK;
/// Altura total do card.
pub const CARD_HEIGHT: f32 = tokens::sizes::SHELF_CARD;

/// Dados de um card de álbum (reduz o número de argumentos da função).
#[derive(Clone, Copy)]
pub struct AlbumCard<'a> {
    /// Artwork quadrado; `None` desenha placeholder.
    pub artwork: Option<&'a egui::TextureHandle>,
    /// Título.
    pub title: &'a str,
    /// Subtítulo.
    pub subtitle: &'a str,
    /// Estado habilitado.
    pub enabled: bool,
    /// Estado selecionado (anel do acento no artwork).
    pub selected: bool,
    /// Largura do artwork/card.
    pub width: f32,
}

/// Desenha o card; estados: hover ilumina o fundo, disabled esmaece,
/// selecionado adiciona anel do acento no artwork.
pub fn album_card(ui: &mut Ui, theme: &Theme, _id: Id, card: AlbumCard<'_>) -> Response {
    let AlbumCard {
        artwork,
        title,
        subtitle,
        enabled,
        selected,
        width,
    } = card;
    let text_zone = CARD_HEIGHT - width;
    let (_, response) = ui.allocate_exact_size(
        Vec2::new(width, width + text_zone),
        Sense::CLICK.union(Sense::FOCUSABLE),
    );
    let state = crate::components::button::derive_state(
        enabled,
        selected,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );

    let painter = ui.painter();
    match state {
        crate::components::button::ButtonState::Hovered => {
            painter.rect_filled(
                response.rect,
                theme::corner_radius(tokens::radius::MD),
                theme.bg_hover,
            );
        }
        crate::components::button::ButtonState::Pressed
        | crate::components::button::ButtonState::Selected => {
            painter.rect_filled(
                response.rect,
                theme::corner_radius(tokens::radius::MD),
                theme.bg_selected,
            );
        }
        _ => {}
    }

    // Artwork com margem de 4 px quando há hover (para respirar dentro do bg).
    let inset = 4.0;
    let art = egui::Rect::from_min_size(
        response.rect.min + Vec2::splat(inset),
        Vec2::splat(width - 2.0 * inset),
    );
    draw_artwork(ui, theme, artwork, art, title);

    if selected {
        ui.painter().rect_stroke(
            art,
            theme::corner_radius(tokens::radius::MD),
            egui::Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Inside,
        );
    }
    if state == crate::components::button::ButtonState::Focused {
        crate::components::button::draw_focus_ring(ui, theme, response.rect, tokens::radius::MD);
    }

    let muted = |c: egui::Color32| egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 120);
    let (title_color, subtitle_color) = if enabled {
        (theme.label_primary, theme.label_secondary)
    } else {
        (muted(theme.label_primary), muted(theme.label_secondary))
    };
    let title_y = art.bottom() + spacing::X8;
    painter.text(
        egui::Pos2::new(art.left(), title_y),
        Align2::LEFT_TOP,
        title,
        typography::font_id(type_scale::TypeToken::Headline),
        title_color,
    );
    painter.text(
        egui::Pos2::new(
            art.left(),
            title_y + type_scale::TypeToken::Headline.spec().line_height,
        ),
        Align2::LEFT_TOP,
        subtitle,
        typography::font_id(type_scale::TypeToken::Subheadline),
        subtitle_color,
    );
    response
}

/// Artwork arredondado ou placeholder de gradiente (para capas ausentes).
pub fn draw_artwork(
    ui: &Ui,
    theme: &Theme,
    artwork: Option<&egui::TextureHandle>,
    rect: egui::Rect,
    seed_label: &str,
) {
    let radius = theme::corner_radius(tokens::radius::MD);
    let painter = ui.painter();
    if let Some(handle) = artwork {
        egui::Image::from_texture(egui::load::SizedTexture::from_handle(handle))
            .corner_radius(tokens::radius::MD)
            .paint_at(ui, rect);
    } else {
        // Placeholder: gradiente de duas tonalidades derivadas do título.
        let hash: u32 = seed_label.bytes().fold(17_u32, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u32::from(b))
        });
        let t = (hash % 100) as f32 / 100.0;
        let base = theme.bg_elevated;
        let top = theme::tint(base, theme.accent, 0.10 + 0.15 * t);
        let bottom = theme::tint(base, theme.accent, 0.02 + 0.06 * (1.0 - t));
        painter.rect_filled(rect, radius, top);
        painter.rect_filled(
            egui::Rect::from_min_size(
                rect.center_bottom() - Vec2::new(rect.width() / 2.0, rect.height() / 2.0),
                Vec2::new(rect.width(), rect.height() / 2.0),
            ),
            radius,
            bottom,
        );
        crate::icons::draw(
            painter,
            crate::icons::Icon::MusicNote,
            rect.center(),
            rect.height() * 0.3,
            theme.label_tertiary,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dimensões fecham com os tokens do design (224 = artwork 180 + 44 texto).
    #[test]
    fn card_dimensions_match_tokens() {
        assert_eq!(CARD_WIDTH, 180.0);
        assert_eq!(CARD_HEIGHT, 224.0);
        assert_eq!(CARD_HEIGHT - CARD_WIDTH, tokens::sizes::LIST_ROW);
    }

    /// Placeholder do título é estável (mesmo título → mesma fração de acento).
    #[test]
    fn placeholder_gradient_is_deterministic_per_title() {
        let fraction = |s: &str| -> u32 {
            s.bytes()
                .fold(17_u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)))
                % 100
        };
        assert_eq!(fraction("Album A"), fraction("Album A"));
        assert_ne!(fraction("Album A"), fraction("Album B"));
    }
}
