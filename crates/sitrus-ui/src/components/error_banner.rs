//! Banner de erro/aviso/informação com opção de fechar.

use egui::{Align2, Id, Response, Sense, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Tipo de banner (usa danger para error, um âmbar para warning, accent para info).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Error,
    Warning,
    Info,
}

impl BannerKind {
    /// Cor de destaque do banner.
    #[must_use]
    pub fn color(self, theme: &Theme) -> egui::Color32 {
        match self {
            BannerKind::Error => theme.danger,
            BannerKind::Warning => egui::Color32::from_rgb(0xFF, 0xC2, 0x5B),
            BannerKind::Info => theme.accent,
        }
    }

    /// Ícone do banner.
    #[must_use]
    pub fn icon(self) -> Icon {
        match self {
            BannerKind::Error | BannerKind::Warning => Icon::Warning,
            BannerKind::Info => Icon::Info,
        }
    }
}

/// Desenha o banner; devolve `Some(Response)` do botão fechar (dismissible).
pub fn error_banner(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    kind: BannerKind,
    message: &str,
    dismissible: bool,
) -> Option<Response> {
    let color = kind.color(theme);
    let height = 48.0;
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        area,
        theme::corner_radius(tokens::radius::MD),
        theme::tint(theme.bg_elevated, color, 0.15),
    );
    painter.rect_stroke(
        area,
        theme::corner_radius(tokens::radius::MD),
        egui::Stroke::new(1.0, theme::tint(color, theme.bg_content, 0.2)),
        egui::StrokeKind::Inside,
    );
    icons::draw(
        painter,
        kind.icon(),
        egui::Pos2::new(area.left() + spacing::X20, area.center().y),
        20.0,
        color,
    );
    painter.text(
        egui::Pos2::new(area.left() + spacing::X40, area.center().y),
        Align2::LEFT_CENTER,
        message,
        typography::font_id(type_scale::TypeToken::Body),
        theme.label_primary,
    );

    if dismissible {
        let close_rect = egui::Rect::from_center_size(
            egui::Pos2::new(area.right() - spacing::X20, area.center().y),
            Vec2::splat(tokens::sizes::MIN_CLICK_TARGET + 4.0),
        );
        icons::draw(
            painter,
            Icon::Close,
            close_rect.center(),
            14.0,
            theme.label_secondary,
        );
        let response = ui.interact(close_rect, id.with("banner_close"), egui::Sense::CLICK);
        Some(response.on_hover_text("Fechar aviso"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada tipo tem cor própria e ícone coerente.
    #[test]
    fn banner_kinds_map_to_colors_and_icons() {
        let theme = Theme::dark();
        assert_eq!(BannerKind::Error.color(&theme), theme.danger);
        assert_ne!(BannerKind::Warning.color(&theme), theme.danger);
        assert_eq!(BannerKind::Info.color(&theme), theme.accent);
        assert_eq!(BannerKind::Error.icon(), Icon::Warning);
        assert_eq!(BannerKind::Info.icon(), Icon::Info);
    }
}
