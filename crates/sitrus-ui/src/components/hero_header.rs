//! Hero (direção B): artwork de 320 px, gradiente do artwork, título Display,
//! subtítulo e botão de tocar.

use egui::{Align2, Id, Rect, Response, Sense, Ui, UiBuilder, Vec2};

use crate::components::button::{self, ButtonContent, ButtonVariant};
use crate::icons::Icon;
use crate::painting;
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Dados do hero.
#[derive(Clone, Debug)]
pub struct HeroData<'a> {
    /// Título (Display 44/700).
    pub title: &'a str,
    /// Subtítulo (Subheadline).
    pub subtitle: Option<&'a str>,
    /// Rótulo do botão de tocar (padrão "Tocar").
    pub action_label: Option<&'a str>,
    /// Gradiente derivado do artwork; sem obra usa os tokens do tema.
    pub gradient: Option<(egui::Color32, egui::Color32)>,
}

impl<'a> HeroData<'a> {
    /// Hero com título e gradiente.
    #[must_use]
    pub fn new(title: &'a str, gradient: Option<(egui::Color32, egui::Color32)>) -> Self {
        Self {
            title,
            subtitle: None,
            action_label: None,
            gradient,
        }
    }
}

/// Desenha o header hero completo; retorna a resposta do botão de tocar.
pub fn hero_header(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    artwork: Option<&egui::TextureHandle>,
    hero: &HeroData<'_>,
    play_enabled: bool,
) -> Response {
    let height = tokens::sizes::HERO;
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();

    // 1) Fundo: gradiente (do artwork ou do tema) + véu escuro.
    let (from, to) = hero
        .gradient
        .unwrap_or((theme.hero_gradient_from, theme.hero_gradient_to));
    painting::gradient_horizontal(painter, area, from, to);
    painting::dark_veil(painter, area);

    // 2) Artwork quadrado de 320 alinhado à esquerda com margem.
    let art_rect =
        Rect::from_min_size(area.min + Vec2::new(spacing::X24, 0.0), Vec2::splat(height));
    match artwork {
        Some(handle) => {
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(handle))
                .corner_radius(tokens::radius::MD)
                .paint_at(ui, art_rect);
        }
        None => {
            painting::placeholder_art(painter, art_rect, to, from);
            crate::icons::draw(
                painter,
                Icon::MusicNote,
                art_rect.center(),
                height * 0.2,
                theme.label_tertiary,
            );
        }
    }

    // 3) Texto: título Display + subtítulo, à direita do artwork.
    let text_pos = egui::Pos2::new(art_rect.right() + spacing::X32, area.center().y - 40.0);
    painter.text(
        text_pos,
        Align2::LEFT_BOTTOM,
        hero.title,
        typography::font_id(type_scale::TypeToken::Display),
        theme.on_accent,
    );
    if let Some(subtitle) = hero.subtitle {
        painter.text(
            text_pos
                + Vec2::new(
                    0.0,
                    type_scale::TypeToken::Display.spec().line_height + spacing::X4,
                ),
            Align2::LEFT_TOP,
            subtitle,
            typography::font_id(type_scale::TypeToken::Subheadline),
            theme::tint(theme.on_accent, theme.bg_content, 0.25),
        );
    }

    // 4) Botão de tocar (primário) abaixo do texto.
    let action_label = hero.action_label.unwrap_or("Tocar");
    let button_rect = Rect::from_min_size(
        egui::Pos2::new(
            art_rect.right() + spacing::X32,
            text_pos.y
                + type_scale::TypeToken::Display.spec().line_height
                + (hero.subtitle.is_some() as u8 as f32)
                    * (type_scale::TypeToken::Subheadline.spec().line_height + spacing::X8)
                + spacing::X16,
        ),
        Vec2::new(140.0, 44.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(id.with("hero_play"))
            .max_rect(button_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    button::button(
        &mut child,
        theme,
        id.with("hero_play"),
        ButtonVariant::Primary,
        ButtonContent::icon_and_text(Icon::Play, action_label),
        play_enabled,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O hero usa 320 px (artwork e altura).
    #[test]
    fn hero_is_320_px() {
        assert_eq!(tokens::sizes::HERO, 320.0);
    }

    /// Botão padrão é "Tocar".
    #[test]
    fn default_action_label_is_tocar() {
        let hero = HeroData::new("Álbum", None);
        assert_eq!(hero.action_label, None);
        assert_eq!(hero.action_label.unwrap_or("Tocar"), "Tocar");
    }
}
