//! Botões: primário, secundário, plain e ícone — com todos os estados
//! (normal, hover, pressed, focus, disabled, selecionado).
//!
//! A matemática de estados é pura ([`derive_state`] e [`visual`]) para ser
//! testável sem contexto gráfico; os wrappers de UI só conectam resposta ↔ estado.

use egui::{Align2, Color32, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Altura dos botões com texto.
pub const TEXT_BUTTON_HEIGHT: f32 = 40.0;
/// Lado de botões de ícone (quadrado).
pub const ICON_BUTTON_SIZE: f32 = 36.0;
/// Lado interno do glifo de ícone.
pub const ICON_GLYPH_SIZE: f32 = 18.0;

/// Variante visual do botão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Ação principal: fundo do acento.
    Primary,
    /// Ação neutra: fundo elevado com borda.
    Secondary,
    /// Texto colorido sem fundo permanente.
    Plain,
    /// Apenas ícone, fundo circular no hover.
    Icon,
}

/// Estados cobertos por todo componente interativo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Hovered,
    Pressed,
    Focused,
    Disabled,
    /// Item marcado como selecionado/toggled.
    Selected,
}

/// Determina o estado a partir das entradas de interação.
///
/// Precedência: disabled > pressed > hovered > selected > focused > normal.
#[must_use]
pub fn derive_state(
    enabled: bool,
    selected: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
) -> ButtonState {
    if !enabled {
        return ButtonState::Disabled;
    }
    if pressed {
        return ButtonState::Pressed;
    }
    if hovered {
        return ButtonState::Hovered;
    }
    if selected {
        return ButtonState::Selected;
    }
    if focused {
        return ButtonState::Focused;
    }
    ButtonState::Normal
}

/// Cores pintadas para uma combinação variante/estado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonVisual {
    /// Cor de fundo; `None` = transparente (plain/icon em repouso).
    pub fill: Option<Color32>,
    /// Cor do texto/ícone.
    pub text: Color32,
    /// Borda opcional (selecionado desenha o acento).
    pub stroke: Option<Stroke>,
}

/// Escolhe as cores da combinação variante/estado (pura).
#[must_use]
pub fn visual(theme: &Theme, variant: ButtonVariant, state: ButtonState) -> ButtonVisual {
    use ButtonState::{Disabled, Focused, Hovered, Normal, Pressed, Selected};
    let muted = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 100);
    match variant {
        ButtonVariant::Primary => match state {
            Normal => ButtonVisual {
                fill: Some(theme.accent),
                text: theme.on_accent,
                stroke: None,
            },
            Focused => ButtonVisual {
                fill: Some(theme.accent),
                text: theme.on_accent,
                stroke: Some(Stroke::new(1.0, theme.focus_ring)),
            },
            Hovered => ButtonVisual {
                fill: Some(theme::tint(theme.accent, Color32::WHITE, 0.10)),
                text: theme.on_accent,
                stroke: None,
            },
            Pressed => ButtonVisual {
                fill: Some(theme.accent_pressed),
                text: theme.on_accent,
                stroke: None,
            },
            Disabled => ButtonVisual {
                fill: Some(theme::tint(theme.accent, theme.bg_content, 0.55)),
                text: muted(theme.on_accent),
                stroke: None,
            },
            Selected => ButtonVisual {
                fill: Some(theme::tint(theme.accent, Color32::WHITE, 0.08)),
                text: theme.on_accent,
                stroke: Some(Stroke::new(2.0, theme.on_accent)),
            },
        },
        ButtonVariant::Secondary => match state {
            Normal => ButtonVisual {
                fill: Some(theme.bg_elevated),
                text: theme.label_primary,
                stroke: Some(Stroke::new(1.0, theme.separator)),
            },
            Focused => ButtonVisual {
                fill: Some(theme.bg_elevated),
                text: theme.label_primary,
                stroke: Some(Stroke::new(1.0, theme.focus_ring)),
            },
            Hovered => ButtonVisual {
                fill: Some(theme.bg_hover),
                text: theme.label_primary,
                stroke: Some(Stroke::new(1.0, theme.separator)),
            },
            Pressed => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.label_primary,
                stroke: Some(Stroke::new(1.0, theme.separator)),
            },
            Disabled => ButtonVisual {
                fill: Some(theme.bg_elevated),
                text: muted(theme.label_secondary),
                stroke: Some(Stroke::new(1.0, theme.separator)),
            },
            Selected => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.accent,
                stroke: Some(Stroke::new(2.0, theme.accent)),
            },
        },
        ButtonVariant::Plain => match state {
            Normal => ButtonVisual {
                fill: None,
                text: theme.accent,
                stroke: None,
            },
            Focused => ButtonVisual {
                fill: None,
                text: theme.accent,
                stroke: Some(Stroke::new(1.0, theme.focus_ring)),
            },
            Hovered => ButtonVisual {
                fill: Some(theme.bg_hover),
                text: theme.accent,
                stroke: None,
            },
            Pressed => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.accent_pressed,
                stroke: None,
            },
            Disabled => ButtonVisual {
                fill: None,
                text: muted(theme.accent),
                stroke: None,
            },
            Selected => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.accent,
                stroke: None,
            },
        },
        ButtonVariant::Icon => match state {
            Normal => ButtonVisual {
                fill: None,
                text: theme.label_secondary,
                stroke: None,
            },
            Focused => ButtonVisual {
                fill: None,
                text: theme.label_secondary,
                stroke: Some(Stroke::new(1.0, theme.focus_ring)),
            },
            Hovered => ButtonVisual {
                fill: Some(theme.bg_hover),
                text: theme.label_primary,
                stroke: None,
            },
            Pressed => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.label_primary,
                stroke: None,
            },
            Disabled => ButtonVisual {
                fill: None,
                text: muted(theme.label_tertiary),
                stroke: None,
            },
            Selected => ButtonVisual {
                fill: Some(theme.bg_selected),
                text: theme.accent,
                stroke: None,
            },
        },
    }
}

/// Conteúdo do botão: texto e/ou ícone.
#[derive(Clone, Copy, Debug)]
pub struct ButtonContent<'a> {
    /// Texto (Headline, 600).
    pub text: Option<&'a str>,
    /// Ícone desenhado à esquerda (ou sozinho).
    pub icon: Option<Icon>,
}

impl<'a> ButtonContent<'a> {
    /// Só texto.
    #[must_use]
    pub fn label(text: &'a str) -> Self {
        Self {
            text: Some(text),
            icon: None,
        }
    }

    /// Só ícone.
    #[must_use]
    pub fn glyph(icon: Icon) -> Self {
        Self {
            text: None,
            icon: Some(icon),
        }
    }

    /// Ícone + texto.
    #[must_use]
    pub fn icon_and_text(icon: Icon, text: &'a str) -> Self {
        Self {
            text: Some(text),
            icon: Some(icon),
        }
    }
}

/// Botão interativo; aloca o próprio retângulo e retorna a resposta.
///
/// `selected` sinaliza item toggled; `enabled: false` pinta e ignora interação
/// visualmente (a resposta ainda existe; chame `.clicked()` só quando enabled).
pub fn button(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    variant: ButtonVariant,
    content: ButtonContent<'_>,
    enabled: bool,
    selected: bool,
) -> Response {
    let icon_only = content.text.is_none();
    let height = if icon_only && variant == ButtonVariant::Icon {
        ICON_BUTTON_SIZE
    } else {
        TEXT_BUTTON_HEIGHT
    };
    let font = typography::font_id(type_scale::TypeToken::Headline);
    let text_width = content
        .text
        .map(|t| {
            ui.painter()
                .layout_no_wrap(t.to_owned(), font, theme.label_primary)
                .size()
                .x
        })
        .unwrap_or(0.0);
    let icon_space = if content.icon.is_some() {
        ICON_GLYPH_SIZE
    } else {
        0.0
    };
    let gap = if content.icon.is_some() && content.text.is_some() {
        spacing::X8
    } else {
        0.0
    };
    let width = if icon_only && variant == ButtonVariant::Icon {
        ICON_BUTTON_SIZE
    } else {
        (spacing::X24 * 2.0 + icon_space + gap + text_width).max(height)
    };

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(width, height),
        Sense::CLICK.union(Sense::FOCUSABLE),
    );
    let response = ui.interact(rect, id, Sense::CLICK.union(Sense::FOCUSABLE));
    let state = derive_state(
        enabled,
        selected,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );
    paint_button(ui, theme, rect, variant, state, content);
    response
}

/// Raio de canto da combinação variante/conteúdo (pílula ou círculo).
#[must_use]
pub fn radius_for(variant: ButtonVariant, content: ButtonContent<'_>) -> f32 {
    match variant {
        ButtonVariant::Icon => ICON_BUTTON_SIZE / 2.0,
        _ => {
            if content.text.is_none_or(str::is_empty) && content.icon.is_some() {
                ICON_BUTTON_SIZE / 2.0
            } else {
                tokens::radius::PILL
            }
        }
    }
}

/// Pinta um botão estático em `rect` (usado pela galeria para inventariar
/// estados sem exigir interação do usuário).
pub fn paint_button(
    ui: &mut Ui,
    theme: &Theme,
    rect: Rect,
    variant: ButtonVariant,
    state: ButtonState,
    content: ButtonContent<'_>,
) {
    let visuals = visual(theme, variant, state);
    let radius = radius_for(variant, content);
    if let Some(fill) = visuals.fill {
        ui.painter()
            .rect_filled(rect, theme::corner_radius(radius), fill);
    }
    if let Some(stroke) = visuals.stroke {
        ui.painter().rect_stroke(
            rect,
            theme::corner_radius(radius),
            stroke,
            StrokeKind::Inside,
        );
    }
    if state == ButtonState::Focused {
        draw_focus_ring(ui, theme, rect, radius);
    }
    paint_label(ui, theme, rect, content, visuals.text);
}

/// Anel de foco (`focusRing`) desenhado fora do widget.
pub fn draw_focus_ring(ui: &Ui, theme: &Theme, rect: Rect, widget_radius: f32) {
    let ring = rect.expand(3.0);
    let ring_radius = (widget_radius + 3.0).min(255.0);
    ui.painter().rect_stroke(
        ring,
        theme::corner_radius(ring_radius),
        Stroke::new(2.0, theme.focus_ring),
        StrokeKind::Outside,
    );
}

fn paint_label(
    ui: &Ui,
    _theme: &Theme,
    rect: Rect,
    content: ButtonContent<'_>,
    text_color: Color32,
) {
    let painter = ui.painter();
    let center = rect.center();
    let font = typography::font_id(type_scale::TypeToken::Headline);
    let label_width = content.text.map(|t| {
        painter
            .layout_no_wrap(t.to_owned(), font, text_color)
            .size()
            .x
    });
    let icon_w = content.icon.map(|_| ICON_GLYPH_SIZE);
    let gap = if content.icon.is_some() && content.text.is_some() {
        spacing::X8
    } else {
        0.0
    };
    let total = icon_w.unwrap_or(0.0) + gap + label_width.unwrap_or(0.0);
    let mut cursor = center.x - total / 2.0;
    if let Some(icon) = content.icon {
        icons::draw(
            painter,
            icon,
            Pos2::new(cursor + ICON_GLYPH_SIZE / 2.0, center.y),
            ICON_GLYPH_SIZE,
            text_color,
        );
        cursor += ICON_GLYPH_SIZE + gap;
    }
    if let Some(text) = content.text {
        painter.text(
            Pos2::new(cursor, center.y),
            Align2::LEFT_CENTER,
            text,
            typography::font_id(type_scale::TypeToken::Headline),
            text_color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DEFAULT_ACCENT;

    /// Transições: precedência disabled > pressed > hovered > selected > focused.
    #[test]
    fn state_precedence_follows_interaction_rules() {
        assert_eq!(
            derive_state(false, false, false, false, false),
            ButtonState::Disabled
        );
        assert_eq!(
            derive_state(false, false, true, true, true),
            ButtonState::Disabled
        );
        assert_eq!(
            derive_state(true, false, true, true, false),
            ButtonState::Pressed
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
            derive_state(true, false, false, false, true),
            ButtonState::Focused
        );
        assert_eq!(
            derive_state(true, false, false, false, false),
            ButtonState::Normal
        );
    }

    /// Cada estado tem pintura distinta nas quatro variantes.
    #[test]
    fn visuals_differ_across_states_and_variants() {
        let theme = Theme::dark();
        let states = [
            ButtonState::Normal,
            ButtonState::Hovered,
            ButtonState::Pressed,
            ButtonState::Focused,
            ButtonState::Disabled,
            ButtonState::Selected,
        ];
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Plain,
            ButtonVariant::Icon,
        ] {
            let visuals: Vec<_> = states
                .iter()
                .map(|state| visual(&theme, variant, *state))
                .collect();
            for (a, state_a) in states.iter().enumerate() {
                for (b, state_b) in states.iter().enumerate().skip(a + 1) {
                    assert_ne!(
                        visuals[a], visuals[b],
                        "{variant:?}: estados {state_a:?} e {state_b:?} são indistintos"
                    );
                }
            }
        }
    }

    /// Desabilitado esmaece o texto (alpha < 255) e escurece o primário.
    #[test]
    fn disabled_visuals_are_muted() {
        let theme = Theme::dark();
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Plain,
            ButtonVariant::Icon,
        ] {
            let disabled = visual(&theme, variant, ButtonState::Disabled);
            assert!(disabled.text.a() < 255, "{variant:?} sem alpha mutado");
        }
        let normal = visual(&theme, ButtonVariant::Primary, ButtonState::Normal)
            .fill
            .expect("primário tem fundo");
        let disabled = visual(&theme, ButtonVariant::Primary, ButtonState::Disabled)
            .fill
            .expect("primário desabilitado também tem fundo");
        let sum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(sum(disabled) < sum(normal), "fundo desabilitado mais fraco");
    }

    /// Selecionado desenha o acento na borda/verde do tema e muda o fundo.
    #[test]
    fn selected_state_paints_accent_ring() {
        let theme = Theme::dark();
        let secondary = visual(&theme, ButtonVariant::Secondary, ButtonState::Selected);
        let stroke = secondary.stroke.expect("secundário selecionado tem borda");
        assert_eq!(stroke.color, theme.accent);
        assert!(stroke.width >= 2.0);
        let plain_selected = visual(&theme, ButtonVariant::Plain, ButtonState::Selected);
        assert_eq!(plain_selected.fill, Some(theme.bg_selected));
    }

    /// Hover clareia o acento; pressed usa `accent_pressed` (token).
    #[test]
    fn primary_hover_and_pressed_match_tokens() {
        let theme = Theme::dark();
        let normal = visual(&theme, ButtonVariant::Primary, ButtonState::Normal);
        assert_eq!(normal.fill, Some(DEFAULT_ACCENT));
        let hovered = visual(&theme, ButtonVariant::Primary, ButtonState::Hovered)
            .fill
            .expect("hover fill");
        let pressed = visual(&theme, ButtonVariant::Primary, ButtonState::Pressed)
            .fill
            .expect("pressed fill");
        assert_ne!(hovered, DEFAULT_ACCENT);
        assert_eq!(pressed, theme.accent_pressed);
    }

    /// Botao de ícone tem raio circular; com texto, pílula.
    #[test]
    fn radius_matches_variant_shape() {
        let theme = Theme::dark();
        assert_eq!(
            radius_for(ButtonVariant::Icon, ButtonContent::glyph(Icon::Play)),
            ICON_BUTTON_SIZE / 2.0
        );
        assert_eq!(
            radius_for(ButtonVariant::Primary, ButtonContent::label("Tocar")),
            tokens::radius::PILL
        );
        let _ = theme;
    }
}
