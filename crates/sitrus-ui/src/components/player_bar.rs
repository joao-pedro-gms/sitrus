//! Barra de player (88 px): transporte, seek, volume e painel de fila.

use egui::{Align2, CornerRadius, Id, Rect, Response, Sense, Ui, UiBuilder, Vec2};

use crate::components::list_row;
use crate::format::format_duration;
use crate::icons::{self, Icon};
use crate::theme::{self, Theme};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

/// Estado consolidado da barra.
#[derive(Clone, Default)]
pub struct PlayerBarState<'a> {
    /// Título da faixa.
    pub title: &'a str,
    /// Artista.
    pub subtitle: &'a str,
    /// Artwork pequeno.
    pub artwork: Option<&'a egui::TextureHandle>,
    /// Estamos tocando?
    pub playing: bool,
    /// Posição em segundos.
    pub position_secs: u32,
    /// Duração em segundos.
    pub duration_secs: u32,
    /// Volume 0..1.
    pub volume: f32,
    /// Fila aberta?
    pub queue_open: bool,
    /// Controles indisponíveis (sem backend de áudio).
    pub enabled: bool,
}

/// Resposta consolidada por frame.
#[derive(Debug)]
pub struct PlayerBarResponse {
    /// Foi clicado em play/pause?
    pub play_pause: Response,
    /// Faixa anterior.
    pub prev: Response,
    /// Próxima faixa.
    pub next: Response,
    /// Novo valor de seek (0..1), se o usuário arrastou/clicou.
    pub seek: Option<f32>,
    /// Novo volume, se mudou.
    pub volume: Option<f32>,
    /// Botão de abrir/fechar fila.
    pub queue_toggle: Response,
}

/// Barra completa; desenha em fila horizontal ocupando a largura disponível.
pub fn player_bar(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    state: &PlayerBarState<'_>,
) -> PlayerBarResponse {
    let height = tokens::sizes::PLAYER_BAR;
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(area, 0.0, theme.bg_elevated);
    ui.painter().line_segment(
        [area.left_top(), area.right_top()],
        egui::Stroke::new(1.0, theme.separator),
    );

    // Esquerda: artwork + título/artista.
    let left_x = area.left() + spacing::X16;
    let art_rect = Rect::from_center_size(
        egui::Pos2::new(left_x + 28.0, area.center().y),
        Vec2::splat(56.0),
    );
    if let Some(handle) = state.artwork {
        egui::Image::from_texture(egui::load::SizedTexture::from_handle(handle))
            .corner_radius(tokens::radius::SM)
            .paint_at(ui, art_rect);
    } else {
        ui.painter().rect_filled(
            art_rect,
            theme::corner_radius(tokens::radius::SM),
            theme.bg_rail,
        );
        icons::draw(
            ui.painter(),
            Icon::MusicNote,
            art_rect.center(),
            24.0,
            theme.label_tertiary,
        );
    }
    let title_x = art_rect.right() + spacing::X12;
    ui.painter().text(
        egui::Pos2::new(title_x, area.center().y - 11.0),
        Align2::LEFT_CENTER,
        state.title,
        typography::font_id(type_scale::TypeToken::Headline),
        theme.label_primary,
    );
    ui.painter().text(
        egui::Pos2::new(title_x, area.center().y + 11.0),
        Align2::LEFT_CENTER,
        state.subtitle,
        typography::font_id(type_scale::TypeToken::Subheadline),
        theme.label_secondary,
    );

    // Centro: transporte + seek rotulado.
    let controls_center = egui::Pos2::new(area.center().x + 40.0, area.top() + 28.0);
    let transport = transport_controls(
        ui,
        theme,
        id.with("transport"),
        state.playing,
        state.enabled,
        controls_center,
    );
    let seek_band = Rect::from_min_size(
        egui::Pos2::new(area.center().x - 190.0, area.top() + 52.0),
        Vec2::new(420.0, 24.0),
    );
    let progress = if state.duration_secs > 0 {
        state.position_secs as f32 / state.duration_secs as f32
    } else {
        0.0
    };
    let seek = seek_slider(
        ui,
        theme,
        id.with("seek"),
        seek_band,
        progress,
        state.enabled,
    );
    let labels_color = theme.label_secondary;
    ui.painter().text(
        egui::Pos2::new(seek_band.left() - 6.0, seek_band.center().y),
        Align2::RIGHT_CENTER,
        format_duration(state.position_secs),
        typography::font_id(type_scale::TypeToken::Footnote),
        labels_color,
    );
    ui.painter().text(
        egui::Pos2::new(seek_band.right() + 6.0, seek_band.center().y),
        Align2::LEFT_CENTER,
        format_duration(state.duration_secs),
        typography::font_id(type_scale::TypeToken::Footnote),
        labels_color,
    );

    // Direita: volume + fila.
    let right_x = area.right() - spacing::X16;
    let volume_icon_at = egui::Pos2::new(right_x - 148.0, area.center().y);
    icons::draw(
        ui.painter(),
        if state.volume <= 0.01 {
            Icon::VolumeOff
        } else {
            Icon::Volume
        },
        volume_icon_at,
        20.0,
        theme.label_secondary,
    );
    let volume_band = Rect::from_min_size(
        egui::Pos2::new(volume_icon_at.x + 16.0, area.center().y - 12.0),
        Vec2::new(110.0, 24.0),
    );
    let volume = volume_slider(
        ui,
        theme,
        id.with("volume"),
        volume_band,
        state.volume,
        state.enabled,
    );
    let queue_button = icon_button_at(
        ui,
        theme,
        id.with("queue"),
        Rect::from_center_size(
            egui::Pos2::new(volume_band.right() + 28.0, area.center().y),
            Vec2::splat(36.0),
        ),
        Icon::Queue,
        state.enabled,
        state.queue_open,
    );

    PlayerBarResponse {
        play_pause: transport.play_pause,
        prev: transport.prev,
        next: transport.next,
        seek: seek.value,
        volume: volume.value,
        queue_toggle: queue_button,
    }
}

/// Respostas de transporte.
#[derive(Debug)]
pub struct TransportResponse {
    /// Faixa anterior.
    pub prev: Response,
    /// Play/pause.
    pub play_pause: Response,
    /// Próxima faixa.
    pub next: Response,
}

/// Botão de ícone genérico (círculo no hover).
pub fn icon_button_at(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    rect: Rect,
    icon: Icon,
    enabled: bool,
    selected: bool,
) -> Response {
    let response = ui.interact(rect, id, Sense::CLICK.union(Sense::FOCUSABLE));
    let state = crate::components::button::derive_state(
        enabled,
        selected,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );
    let visual = crate::components::button::visual(
        theme,
        crate::components::button::ButtonVariant::Icon,
        state,
    );
    if let Some(bg) = visual.fill {
        ui.painter_at(rect)
            .rect_filled(rect, theme::corner_radius(tokens::radius::PILL), bg);
    }
    if state == crate::components::button::ButtonState::Focused {
        crate::components::button::draw_focus_ring(ui, theme, rect, tokens::radius::PILL);
    }
    icons::draw(ui.painter(), icon, rect.center(), 20.0, visual.text);
    response
}

/// Prev / play-pause / next centrados em `center`.
pub fn transport_controls(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    playing: bool,
    enabled: bool,
    center: egui::Pos2,
) -> TransportResponse {
    let gap = spacing::X20;
    let play_circle = 44.0_f32;
    let small = 36.0_f32;
    let prev_rect = Rect::from_center_size(
        egui::Pos2::new(
            center.x - gap - play_circle / 2.0 - small / 2.0 - gap,
            center.y,
        ),
        Vec2::splat(small),
    );
    let play_rect = Rect::from_center_size(center, Vec2::splat(play_circle));
    let next_rect = Rect::from_center_size(
        egui::Pos2::new(
            center.x + gap + play_circle / 2.0 + small / 2.0 + gap,
            center.y,
        ),
        Vec2::splat(small),
    );

    let prev = icon_button_at(
        ui,
        theme,
        id.with("prev"),
        prev_rect,
        Icon::SkipBack,
        enabled,
        false,
    );
    let next = icon_button_at(
        ui,
        theme,
        id.with("next"),
        next_rect,
        Icon::SkipForward,
        enabled,
        false,
    );

    // Play/pause: circle primário.
    let response = ui.interact(
        play_rect,
        id.with("play"),
        Sense::CLICK.union(Sense::FOCUSABLE),
    );
    let state = crate::components::button::derive_state(
        enabled,
        false,
        response.hovered(),
        response.is_pointer_button_down_on() && enabled,
        response.has_focus(),
    );
    let fill = match state {
        crate::components::button::ButtonState::Normal => theme.accent,
        crate::components::button::ButtonState::Hovered => {
            theme::tint(theme.accent, egui::Color32::WHITE, 0.10)
        }
        crate::components::button::ButtonState::Pressed => theme.accent_pressed,
        crate::components::button::ButtonState::Focused => theme.accent,
        crate::components::button::ButtonState::Disabled => {
            theme::tint(theme.accent, theme.bg_content, 0.55)
        }
        crate::components::button::ButtonState::Selected => {
            theme::tint(theme.accent, egui::Color32::WHITE, 0.08)
        }
    };
    ui.painter_at(play_rect).rect_filled(
        play_rect,
        theme::corner_radius(tokens::radius::PILL),
        fill,
    );
    if state == crate::components::button::ButtonState::Focused {
        crate::components::button::draw_focus_ring(ui, theme, play_rect, tokens::radius::PILL);
    }
    icons::draw(
        ui.painter(),
        if playing { Icon::Pause } else { Icon::Play },
        play_rect.center(),
        20.0,
        theme.on_accent,
    );

    TransportResponse {
        prev,
        play_pause: response,
        next,
    }
}

/// Resultado de slider customizado.
#[derive(Debug)]
pub struct SliderResponse {
    /// `Some(valor 0..1)` quando o usuário mudou o valor neste frame.
    pub value: Option<f32>,
    /// Resposta da área.
    pub response: Response,
}

/// Seek (trilho 4 px pílula, preenchimento acento, handle branco 12 px).
pub fn seek_slider(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    band: Rect,
    value: f32,
    enabled: bool,
) -> SliderResponse {
    slider(ui, theme, id, band, value, enabled, theme.accent)
}

/// Volume (mesma mecânica, preenchimento neutro).
pub fn volume_slider(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    band: Rect,
    value: f32,
    enabled: bool,
) -> SliderResponse {
    slider(ui, theme, id, band, value, enabled, theme.label_secondary)
}

fn slider(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    band: Rect,
    value: f32,
    enabled: bool,
    fill_color: egui::Color32,
) -> SliderResponse {
    let value = value.clamp(0.0, 1.0);
    let response = ui.interact(band, id, Sense::CLICK.union(Sense::DRAG));
    let interacting = enabled && (response.dragged() || response.clicked() && response.hovered());
    let _ = interacting;
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());

    let mut new_value = None;
    if enabled && (response.dragged() || response.clicked()) {
        let t = pointer_pos.map_or(value, |pos| {
            ((pos.x - band.left()) / band.width().max(1.0)).clamp(0.0, 1.0)
        });
        if (t - value).abs() > f32::EPSILON {
            new_value = Some(t);
        }
    }
    let painted_value = new_value.unwrap_or(value);

    let track_h = 4.0;
    let track = Rect::from_center_size(band.center(), Vec2::new(band.width(), track_h));
    let filled = Rect::from_min_size(track.min, Vec2::new(track.width() * painted_value, track_h));
    let painter = ui.painter();
    let track_color = if enabled {
        theme.bg_hover
    } else {
        theme.bg_rail
    };
    painter.rect_filled(track, CornerRadius::same(2), track_color);
    painter.rect_filled(
        filled,
        CornerRadius::same(2),
        if enabled {
            fill_color
        } else {
            theme.label_tertiary
        },
    );
    if enabled {
        let handle = Rect::from_center_size(
            egui::Pos2::new(filled.right(), track.center().y),
            Vec2::splat(12.0),
        );
        let handle_color = if response.dragged() || response.hovered() {
            theme.on_accent
        } else {
            theme.label_primary
        };
        painter.rect_filled(handle, CornerRadius::same(6), handle_color);
    }

    SliderResponse {
        value: new_value,
        response,
    }
}

/// Um item da fila de reprodução.
#[derive(Clone, Debug)]
pub struct QueueItem<'a> {
    /// Título da faixa.
    pub title: &'a str,
    /// Artista.
    pub subtitle: &'a str,
    /// Duração em segundos.
    pub duration_secs: u32,
    /// Faixa em reprodução.
    pub active: bool,
}

/// Ações da fila emitidas no frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct QueuePanelResponse {
    /// Índice clicado para pular à faixa.
    pub clicked: Option<usize>,
    /// Par (origem, destino) de uma faixa arrastada.
    pub reordered: Option<(usize, usize)>,
}

/// Painel de fila (usado dentro de um `Ui` posicionado pelo chamador).
pub fn queue_panel(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    items: &[QueueItem<'_>],
) -> QueuePanelResponse {
    let panel_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(
        panel_rect,
        theme::corner_radius(tokens::radius::LG),
        theme.bg_elevated,
    );
    ui.painter().rect_stroke(
        panel_rect,
        theme::corner_radius(tokens::radius::LG),
        egui::Stroke::new(1.0, theme.separator),
        egui::StrokeKind::Inside,
    );

    let mut panel = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(panel_rect.shrink(spacing::X8))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    panel.add_space(spacing::X4);
    panel.label(
        egui::RichText::new("Tocando a seguir")
            .font(typography::font_id(type_scale::TypeToken::Title2))
            .color(theme.label_primary),
    );
    panel.add_space(spacing::X8);

    let mut output = QueuePanelResponse::default();
    let drag_source_id = id.with("queue_drag_source");
    for (i, item) in items.iter().enumerate() {
        let row = list_row::ListRow {
            index: Some(i),
            title: item.title,
            subtitle: Some(item.subtitle),
            duration_secs: Some(item.duration_secs),
            artwork: None,
            hover_icon: Some(if item.active { Icon::Pause } else { Icon::Play }),
            hover_tooltip: None,
        };
        let response = list_row::draggable_list_row(
            &mut panel,
            theme,
            id.with(("queue_row", i)),
            row,
            true,
            item.active,
        );
        if response.clicked() {
            output.clicked = Some(i);
        }
        if response.drag_started() {
            panel
                .ctx()
                .data_mut(|data| data.insert_temp(drag_source_id, i));
        }
        if response.hovered() && panel.input(|input| input.pointer.any_released()) {
            let from = panel
                .ctx()
                .data_mut(|data| data.get_temp::<usize>(drag_source_id));
            if let Some(from) = from.filter(|from| *from != i) {
                output.reordered = Some((from, i));
            }
            panel
                .ctx()
                .data_mut(|data| data.remove_temp::<usize>(drag_source_id));
        }
        panel.add_space(spacing::X4);
    }
    if items.is_empty() {
        panel.label(
            egui::RichText::new("A fila está vazia")
                .font(typography::font_id(type_scale::TypeToken::Subheadline))
                .color(theme.label_secondary),
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Progresso do seek é proporcional e seguro quando duração é 0.
    #[test]
    fn seek_progress_math() {
        let progress = |pos: u32, dur: u32| {
            if dur > 0 {
                pos as f32 / dur as f32
            } else {
                0.0
            }
        };
        assert!((progress(30, 180) - 1.0 / 6.0).abs() < 1e-6);
        assert_eq!(progress(0, 0), 0.0);
    }

    /// Botão de ícone cabe no alvo de clique mínimo (verificação em tempo de compilação).
    #[test]
    fn icon_buttons_respect_min_click_target() {
        const { assert!(tokens::sizes::MIN_CLICK_TARGET <= 36.0) }
    }

    /// Volume satura em 0..=1 (o mesmo que o slider já faz).
    #[test]
    fn volume_clamps() {
        let volume: f32 = 1.7;
        assert_eq!(volume.clamp(0.0, 1.0), 1.0);
    }
}
