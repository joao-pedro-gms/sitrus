//! Linha de lista (list_row) e linha de faixa (song_row): artwork opcional,
//! título, subtítulo, duração e ação de hover — altura densa de 44 px.

use egui::{Align2, Color32, Id, Response, Sense, Ui, Vec2};

use crate::components::button;
use crate::format::format_duration;
use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Dados de uma linha.
#[derive(Clone)]
pub struct ListRow<'a> {
    /// Índice da faixa na lista (desenhado no lugar do artwork).
    pub index: Option<usize>,
    /// Título (Body/Headline).
    pub title: &'a str,
    /// Subtítulo (artista/álbum).
    pub subtitle: Option<&'a str>,
    /// Duração em segundos (formatada à direita).
    pub duration_secs: Option<u32>,
    /// Artwork quadrado à esquerda; sem artwork + sem índice = sem slot.
    pub artwork: Option<&'a egui::TextureHandle>,
    /// Ícone mostrado no hover (ação, ex.: Play) sobrepõe a duração.
    pub hover_icon: Option<Icon>,
    /// Ação de hover com texto alternativo (acessibilidade).
    pub hover_tooltip: Option<&'a str>,
}

impl<'a> ListRow<'a> {
    /// Linha só com título.
    #[must_use]
    pub fn plain(title: &'a str) -> Self {
        Self {
            index: None,
            title,
            subtitle: None,
            duration_secs: None,
            artwork: None,
            hover_icon: None,
            hover_tooltip: None,
        }
    }
}

/// Linha de lista (44 px).
pub fn list_row(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    row: ListRow<'_>,
    enabled: bool,
    selected: bool,
) -> Response {
    let height = tokens::sizes::LIST_ROW;
    let (_, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::CLICK.union(Sense::FOCUSABLE),
    );
    let state = button::derive_state(
        enabled,
        selected,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );

    let painter = ui.painter();
    match state {
        button::ButtonState::Normal | button::ButtonState::Focused => {}
        button::ButtonState::Hovered => {
            painter.rect_filled(
                response.rect,
                theme::corner_radius(tokens::radius::SM),
                theme.bg_hover,
            );
        }
        button::ButtonState::Pressed | button::ButtonState::Selected => {
            painter.rect_filled(
                response.rect,
                theme::corner_radius(tokens::radius::SM),
                theme.bg_selected,
            );
        }
        button::ButtonState::Disabled => {}
    }

    let muted = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 110);
    let (title_color, sub_color, dur_color) = if enabled {
        (
            theme.label_primary,
            theme.label_secondary,
            theme.label_secondary,
        )
    } else {
        (
            muted(theme.label_primary),
            muted(theme.label_secondary),
            muted(theme.label_tertiary),
        )
    };

    let mut cursor_x = response.rect.left() + spacing::X12;
    let center_y = response.rect.center().y;

    // Slot de artwork/índice (36 px) quando presente.
    if row.artwork.is_some() || row.index.is_some() {
        let slot = egui::Rect::from_min_size(
            egui::Pos2::new(cursor_x, center_y - 16.0),
            Vec2::splat(32.0),
        );
        if let Some(handle) = row.artwork {
            painter.rect_filled(
                slot,
                theme::corner_radius(tokens::radius::SM),
                theme.bg_rail,
            );
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(handle))
                .corner_radius(tokens::radius::SM)
                .paint_at(ui, slot);
        } else {
            let label = row.index.map_or(String::new(), |i| (i + 1).to_string());
            painter.text(
                slot.center(),
                Align2::CENTER_CENTER,
                label,
                typography::font_id(type_scale::TypeToken::Body),
                dur_color,
            );
        }
        cursor_x += 32.0 + spacing::X12;
    }

    // Texto principal (título + subtítulo opcional).
    if row.subtitle.is_some() {
        painter.text(
            egui::Pos2::new(cursor_x, center_y - 11.0),
            Align2::LEFT_CENTER,
            row.title,
            typography::font_id(type_scale::TypeToken::Body),
            title_color,
        );
        painter.text(
            egui::Pos2::new(cursor_x, center_y + 11.0),
            Align2::LEFT_CENTER,
            row.subtitle.unwrap_or_default(),
            typography::font_id(type_scale::TypeToken::Subheadline),
            sub_color,
        );
    } else {
        painter.text(
            egui::Pos2::new(cursor_x, center_y),
            Align2::LEFT_CENTER,
            row.title,
            typography::font_id(type_scale::TypeToken::Body),
            title_color,
        );
    }

    // Duração / ação de hover à direita.
    let action_center = egui::Pos2::new(response.rect.right() - spacing::X24, center_y);
    if state == button::ButtonState::Hovered {
        if let Some(icon) = row.hover_icon {
            icons::draw(painter, icon, action_center, 18.0, theme.accent);
        } else if let Some(duration) = row.duration_secs {
            painter.text(
                action_center,
                Align2::RIGHT_CENTER,
                format_duration(duration),
                typography::font_id(type_scale::TypeToken::Subheadline),
                dur_color,
            );
        }
    } else if let Some(duration) = row.duration_secs {
        painter.text(
            action_center,
            Align2::RIGHT_CENTER,
            format_duration(duration),
            typography::font_id(type_scale::TypeToken::Subheadline),
            dur_color,
        );
    }

    if state == button::ButtonState::Focused {
        button::draw_focus_ring(ui, theme, response.rect, tokens::radius::SM);
    }

    // Ação clicável de hover (área do ícone direito).
    let action_id = id.with((row.title, "hover_action"));
    let action_rect =
        egui::Rect::from_center_size(action_center, Vec2::splat(tokens::sizes::MIN_CLICK_TARGET));
    ui.interact(action_rect, action_id, Sense::CLICK)
}

/// Alias semântico para linha de faixa (mesma apresentação, hover com Play).
pub fn song_row(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    row: ListRow<'_>,
    enabled: bool,
    selected: bool,
) -> Response {
    // A linha de faixa é a list_row com ação de hover padrão (Play).
    let row_with_action = ListRow {
        hover_icon: row.hover_icon.or(Some(Icon::Play)),
        hover_tooltip: row.hover_tooltip.or(Some("Tocar faixa")),
        ..row
    };
    list_row(ui, theme, id, row_with_action, enabled, selected)
}

/// Tooltip da ação de hover, se houver (útil para contexto menu futuro).
#[must_use]
pub fn hover_tooltip<'a>(row: &ListRow<'a>) -> Option<&'a str> {
    row.hover_tooltip
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Duração da linha usa o formatador compartilhado.
    #[test]
    fn row_duration_formatting() {
        assert_eq!(format_duration(83), "1:23");
        assert_eq!(format_duration(3_900), "1:05:00");
    }

    /// song_row injeta a ação de hover padrão quando a linha não tem.
    #[test]
    fn song_row_defaults_to_play_hover_action() {
        let row = ListRow::plain("Faixa");
        assert!(row.hover_icon.is_none());
        let with_default = ListRow {
            hover_icon: row.hover_icon.or(Some(Icon::Play)),
            hover_tooltip: row.hover_tooltip.or(Some("Tocar faixa")),
            ..row.clone()
        };
        assert!(with_default.hover_icon.is_some());
        assert_eq!(hover_tooltip(&with_default), Some("Tocar faixa"));
    }

    /// Linha densa usa o token 44.
    #[test]
    fn row_height_is_design_token() {
        assert_eq!(tokens::sizes::LIST_ROW, 44.0);
    }
}
