//! Paletas semânticas do Sitrus (papéis definidos em `docs/design-system.md`).
//!
//! Os componentes nunca usam cores soltas: leem os papéis de [`Theme`].
//! O tema escuro é o padrão; o claro é a variante derivada do mesmo conjunto
//! de papéis. O acento padrão é o verde-vermelho de música `#FA243C` e é
//! configurável via [`Theme::with_accent`].

use egui::{Color32, Context, Style, Ui};

/// Acento padrão do app: vermelho-música `#FA243C`.
pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0xFA, 0x24, 0x3C);

/// Papéis de cor do design system, para os dois temas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// `true` para a variante escura (padrão).
    pub dark_mode: bool,
    /// `bg.window`: fundo da janela/rail.
    pub bg_window: Color32,
    /// `bg.content`: fundo das áreas de conteúdo.
    pub bg_content: Color32,
    /// `bg.rail`: superfície do rail de 72 px.
    pub bg_rail: Color32,
    /// `bg.elevated`: cartões, player, modais.
    pub bg_elevated: Color32,
    /// `bg.hover`: passe do mouse sobre listas/cartões.
    pub bg_hover: Color32,
    /// `bg.selected`: item selecionado.
    pub bg_selected: Color32,
    /// `label.primary`: texto principal.
    pub label_primary: Color32,
    /// `label.secondary` (60%).
    pub label_secondary: Color32,
    /// `label.tertiary` (35%).
    pub label_tertiary: Color32,
    /// `separator` (branco 10% / preto 8%).
    pub separator: Color32,
    /// `accent` (padrão `#FA243C`, configurável).
    pub accent: Color32,
    /// `accent.pressed`: derivado do acento, mais escuro.
    pub accent_pressed: Color32,
    /// `onAccent`: texto/ícone sobre o acento.
    pub on_accent: Color32,
    /// `hero.gradient.from`: fallback sem artwork.
    pub hero_gradient_from: Color32,
    /// `hero.gradient.to`: fallback sem artwork.
    pub hero_gradient_to: Color32,
    /// `danger`.
    pub danger: Color32,
    /// `success`.
    pub success: Color32,
    /// `focusRing`.
    pub focus_ring: Color32,
}

impl Theme {
    /// Tema escuro, padrão do app.
    #[must_use]
    pub fn dark() -> Self {
        let content = Color32::from_rgb(0x1A, 0x1A, 0x20);
        let white = Color32::WHITE;
        Self {
            dark_mode: true,
            bg_window: Color32::from_rgb(0x12, 0x12, 0x16),
            bg_content: content,
            bg_rail: Color32::from_rgb(0x13, 0x13, 0x18),
            bg_elevated: Color32::from_rgb(0x24, 0x24, 0x2C),
            bg_hover: tint(content, white, 0.08),
            bg_selected: tint(content, white, 0.14),
            label_primary: Color32::from_rgb(0xF2, 0xF2, 0xF7),
            label_secondary: Color32::from_rgb(0x9A, 0x9A, 0x9E),
            label_tertiary: Color32::from_rgb(0x5B, 0x5B, 0x60),
            separator: tint(content, white, 0.10),
            accent: DEFAULT_ACCENT,
            accent_pressed: derive_pressed(DEFAULT_ACCENT),
            on_accent: Color32::WHITE,
            hero_gradient_from: Color32::from_rgb(0x3A, 0x3A, 0x46),
            hero_gradient_to: Color32::from_rgb(0x1A, 0x1A, 0x22),
            danger: Color32::from_rgb(0xFF, 0x45, 0x3A),
            success: Color32::from_rgb(0x30, 0xD1, 0x58),
            focus_ring: Color32::from_rgb(0x0A, 0x84, 0xFF),
        }
    }

    /// Tema claro, variante derivada dos mesmos papéis.
    #[must_use]
    pub fn light() -> Self {
        let content = Color32::WHITE;
        let black = Color32::BLACK;
        Self {
            dark_mode: false,
            bg_window: Color32::from_rgb(0xF6, 0xF6, 0xF8),
            bg_content: content,
            bg_rail: Color32::from_rgb(0xF0, 0xF0, 0xF4),
            bg_elevated: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            bg_hover: tint(content, black, 0.05),
            bg_selected: tint(content, DEFAULT_ACCENT, 0.10),
            label_primary: Color32::from_rgb(0x14, 0x14, 0x16),
            label_secondary: Color32::from_rgb(0x66, 0x66, 0x6A),
            label_tertiary: Color32::from_rgb(0xA6, 0xA6, 0xAA),
            separator: tint(content, black, 0.08),
            accent: DEFAULT_ACCENT,
            accent_pressed: derive_pressed(DEFAULT_ACCENT),
            on_accent: Color32::WHITE,
            hero_gradient_from: Color32::from_rgb(0xE4, 0xE4, 0xEA),
            hero_gradient_to: Color32::from_rgb(0xC7, 0xC7, 0xD1),
            danger: Color32::from_rgb(0xFF, 0x3B, 0x30),
            success: Color32::from_rgb(0x34, 0xC7, 0x59),
            focus_ring: Color32::from_rgb(0x00, 0x7A, 0xFF),
        }
    }

    /// Substitui o acento; `accent.pressed` é rederivado como mais escuro.
    #[must_use]
    pub fn with_accent(mut self, accent: Color32) -> Self {
        self.accent = accent;
        self.accent_pressed = derive_pressed(accent);
        self
    }

    /// Aplica os papéis ao estilo egui do `ui` atual (para widgets egui
    /// de apoio herarem o tema dentro do escopo do componente).
    pub fn apply_ui(&self, ui: &mut Ui) {
        ui.style_mut().visuals = self.to_visuals();
    }

    /// Converte os papéis para `egui::Visuals`.
    #[must_use]
    pub fn to_visuals(&self) -> egui::Visuals {
        let stroke_of = |color: Color32, w: f32| egui::Stroke::new(w, color);
        let widget_of = |weak_bg: Color32, fg: Color32| egui::style::WidgetVisuals {
            bg_fill: self.bg_elevated,
            weak_bg_fill: weak_bg,
            bg_stroke: stroke_of(self.separator, 1.0),
            corner_radius: corner_radius(crate::tokens::radius::MD),
            fg_stroke: stroke_of(fg, 1.0),
            expansion: 0.0,
        };
        egui::Visuals {
            dark_mode: self.dark_mode,
            panel_fill: self.bg_content,
            window_fill: self.bg_window,
            faint_bg_color: self.bg_hover,
            extreme_bg_color: self.bg_elevated,
            text_edit_bg_color: Some(self.bg_elevated),
            override_text_color: Some(self.label_primary),
            hyperlink_color: self.accent,
            error_fg_color: self.danger,
            warn_fg_color: Color32::from_rgb(0xFF, 0xC2, 0x5B),
            selection: egui::style::Selection {
                bg_fill: self.bg_selected,
                stroke: stroke_of(self.accent, 1.0),
            },
            weak_text_color: Some(self.label_secondary),
            window_corner_radius: corner_radius(crate::tokens::radius::LG),
            window_stroke: stroke_of(self.separator, 1.0),
            widgets: egui::style::Widgets {
                noninteractive: widget_of(self.bg_elevated, self.label_secondary),
                inactive: widget_of(self.bg_elevated, self.label_primary),
                hovered: widget_of(self.bg_hover, self.label_primary),
                active: widget_of(self.bg_selected, self.label_primary),
                open: widget_of(self.bg_elevated, self.label_primary),
            },
            ..egui::Visuals::default()
        }
    }

    /// Instala o tema no `Context` inteiro (para a janela principal).
    pub fn apply(self, ctx: &Context) {
        ctx.all_styles_mut(|style| {
            *style = self.into_style(Style::default());
        });
    }

    /// Versão completa de [`Theme::to_visuals`] para um `Style` novo.
    #[must_use]
    pub fn into_style(&self, mut style: Style) -> Style {
        style.visuals = self.to_visuals();
        style.spacing.item_spacing =
            egui::vec2(crate::tokens::spacing::X8, crate::tokens::spacing::X8);
        style
    }

    /// Cor de hierarquia de label pela posição 0/1/2.
    #[must_use]
    pub fn label(&self, level: LabelLevel) -> Color32 {
        match level {
            LabelLevel::Primary => self.label_primary,
            LabelLevel::Secondary => self.label_secondary,
            LabelLevel::Tertiary => self.label_tertiary,
        }
    }
}

/// Hierarquia de rótulos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelLevel {
    Primary,
    Secondary,
    Tertiary,
}

/// Deriva `accent.pressed` escurecendo o acento (mistura com preto 18%).
#[must_use]
pub fn derive_pressed(accent: Color32) -> Color32 {
    tint(accent, Color32::BLACK, 0.18)
}

/// Converte um raio token em `egui::CornerRadius` (satura em 255 px,
/// o máximo do tipo — o raio de pílula 999 vira canto plenamente redondo).
#[must_use]
pub fn corner_radius(value: f32) -> egui::CornerRadius {
    let clamped = value.clamp(0.0, 255.0);
    egui::CornerRadius {
        nw: clamped as u8,
        ne: clamped as u8,
        sw: clamped as u8,
        se: clamped as u8,
    }
}

/// Mistura `base` com `overlay` na transparência indicada.
#[must_use]
pub fn tint(base: Color32, overlay: Color32, alpha: f32) -> Color32 {
    let a = alpha.clamp(0.0, 1.0);
    let mixed = |x: u8, y: u8| (f32::from(x) * (1.0 - a) + f32::from(y) * a).round() as u8;
    Color32::from_rgb(
        mixed(base.r(), overlay.r()),
        mixed(base.g(), overlay.g()),
        mixed(base.b(), overlay.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O tema escuro é o padrão e usa superfícies mais escuras que o claro.
    #[test]
    fn dark_is_default_and_darker_than_light() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert!(dark.dark_mode);
        assert!(!light.dark_mode);
        let sum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(sum(dark.bg_content) < sum(light.bg_content));
        assert!(sum(dark.bg_window) < sum(light.bg_window));
    }

    /// Papéis presentes nos dois temas (nada fica `TRANSPARENT` por engano).
    #[test]
    fn both_palettes_fill_every_role() {
        for theme in [Theme::dark(), Theme::light()] {
            for (name, color) in [
                ("bg_window", theme.bg_window),
                ("bg_content", theme.bg_content),
                ("bg_rail", theme.bg_rail),
                ("bg_elevated", theme.bg_elevated),
                ("bg_hover", theme.bg_hover),
                ("bg_selected", theme.bg_selected),
                ("label_primary", theme.label_primary),
                ("label_secondary", theme.label_secondary),
                ("label_tertiary", theme.label_tertiary),
                ("separator", theme.separator),
                ("accent", theme.accent),
                ("on_accent", theme.on_accent),
                ("danger", theme.danger),
                ("success", theme.success),
                ("focus_ring", theme.focus_ring),
            ] {
                assert_ne!(color, Color32::TRANSPARENT, "papel {name} vazio");
            }
            // Hierarquia de labels escurece clareando (escuro) ou escurecendo (claro).
            let sum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
            if theme.dark_mode {
                assert!(sum(theme.label_secondary) < sum(theme.label_primary));
                assert!(sum(theme.label_tertiary) < sum(theme.label_secondary));
            } else {
                assert!(sum(theme.label_secondary) > sum(theme.label_primary));
                assert!(sum(theme.label_tertiary) > sum(theme.label_secondary));
            }
        }
    }

    /// O acento padrão é `#FA243C` e é configurável com `pressed` rederivado.
    #[test]
    fn accent_is_configurable_and_pressed_is_rederived() {
        let theme = Theme::dark();
        assert_eq!(theme.accent, DEFAULT_ACCENT);

        let custom = Color32::from_rgb(0, 200, 255);
        let themed = Theme::dark().with_accent(custom);
        assert_eq!(themed.accent, custom);
        assert_eq!(themed.accent_pressed, derive_pressed(custom));
        let sum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(
            sum(themed.accent_pressed) < sum(custom),
            "pressed é mais escuro"
        );
    }

    /// `tint` interpola e satura em 0..1 sem estourar canais.
    #[test]
    fn tint_interpolates_between_bases() {
        assert_eq!(tint(Color32::BLACK, Color32::WHITE, 0.0), Color32::BLACK);
        assert_eq!(tint(Color32::BLACK, Color32::WHITE, 1.0), Color32::WHITE);
        assert_eq!(
            tint(Color32::BLACK, Color32::WHITE, 0.5),
            Color32::from_rgb(128, 128, 128)
        );
        // Alpha fora da faixa satura em vez de estourar.
        assert_eq!(tint(Color32::RED, Color32::BLUE, 2.0), Color32::BLUE);
    }
}
