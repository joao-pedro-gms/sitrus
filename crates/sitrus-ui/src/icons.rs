//! Ícones desenhados com primitivas do painter (desenho original, sem SF
//! Symbols e sem dependência de fonte de ícones).
//!
//! Todos os ícones são desenhados centrados em `Pos2` com tamanho lógico
//! `size` e cor única, para herar estados dos componentes.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

/// Ícones usados pelos componentes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Faixa anterior.
    SkipBack,
    /// Próxima faixa.
    SkipForward,
    /// Busca (lupa).
    Search,
    /// Seta para a esquerda.
    ChevronLeft,
    /// Seta para a direita.
    ChevronRight,
    /// Seta para baixo.
    ChevronDown,
    /// Fechar (X).
    Close,
    /// Adicionar.
    Add,
    /// Concluído (check).
    Check,
    /// Lista (linhas).
    List,
    /// Grade (capas).
    Grid,
    /// Fila de reprodução.
    Queue,
    /// Volume ativo.
    Volume,
    /// Volume silenciado.
    VolumeOff,
    /// Nota musical (estados vazios).
    MusicNote,
    /// Aviso (banners, toasts de erro).
    Warning,
    /// Informação.
    Info,
}

/// Desenha o ícone centrado em `center`.
pub fn draw(painter: &Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let stroke_width = (size * 0.10).clamp(1.5, 3.5);
    let stroke = Stroke::new(stroke_width, color);
    let fill = color;
    match icon {
        Icon::Play => {
            let p1 = Pos2::new(center.x - 0.22 * size, center.y - 0.34 * size);
            let p2 = Pos2::new(center.x + 0.34 * size, center.y);
            let p3 = Pos2::new(center.x - 0.22 * size, center.y + 0.34 * size);
            painter.add(Shape::convex_polygon(vec![p1, p2, p3], fill, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-0.13_f32, 0.13] {
                let bar = Rect::from_center_size(
                    Pos2::new(center.x + dx * size, center.y),
                    Vec2::new(0.16 * size, 0.66 * size),
                );
                painter.rect_filled(bar, CornerRadius::same(2), fill);
            }
        }
        Icon::SkipBack => skip_icon(painter, center, size, fill, false),
        Icon::SkipForward => skip_icon(painter, center, size, fill, true),
        Icon::Search => {
            let lens = Pos2::new(center.x - 0.06 * size, center.y - 0.06 * size);
            painter.circle_stroke(lens, 0.28 * size, stroke);
            let from = Pos2::new(center.x + 0.15 * size, center.y + 0.15 * size);
            let to = Pos2::new(center.x + 0.38 * size, center.y + 0.38 * size);
            painter.line_segment([from, to], stroke);
        }
        Icon::ChevronLeft | Icon::ChevronRight | Icon::ChevronDown => {
            let points = match icon {
                Icon::ChevronLeft => vec![
                    Pos2::new(center.x + 0.12 * size, center.y - 0.28 * size),
                    Pos2::new(center.x - 0.14 * size, center.y),
                    Pos2::new(center.x + 0.12 * size, center.y + 0.28 * size),
                ],
                Icon::ChevronRight => vec![
                    Pos2::new(center.x - 0.12 * size, center.y - 0.28 * size),
                    Pos2::new(center.x + 0.14 * size, center.y),
                    Pos2::new(center.x - 0.12 * size, center.y + 0.28 * size),
                ],
                _ => vec![
                    Pos2::new(center.x - 0.28 * size, center.y - 0.12 * size),
                    Pos2::new(center.x, center.y + 0.14 * size),
                    Pos2::new(center.x + 0.28 * size, center.y - 0.12 * size),
                ],
            };
            painter.add(Shape::line(points, stroke));
        }
        Icon::Close => {
            let d = 0.30 * size;
            painter.line_segment(
                [
                    Pos2::new(center.x - d, center.y - d),
                    Pos2::new(center.x + d, center.y + d),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - d, center.y + d),
                    Pos2::new(center.x + d, center.y - d),
                ],
                stroke,
            );
        }
        Icon::Add => {
            let d = 0.30 * size;
            painter.line_segment(
                [
                    Pos2::new(center.x - d, center.y),
                    Pos2::new(center.x + d, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - d),
                    Pos2::new(center.x, center.y + d),
                ],
                stroke,
            );
        }
        Icon::Check => {
            let points = vec![
                Pos2::new(center.x - 0.32 * size, center.y + 0.02 * size),
                Pos2::new(center.x - 0.08 * size, center.y + 0.26 * size),
                Pos2::new(center.x + 0.34 * size, center.y - 0.24 * size),
            ];
            painter.add(Shape::line(points, stroke));
        }
        Icon::List => {
            for (i, dy) in [-0.26_f32, 0.0, 0.26].iter().enumerate() {
                let y = center.y + dy * size;
                let x0 = center.x - 0.34 * size;
                let x1 = if i == 1 {
                    center.x + 0.34 * size
                } else {
                    center.x + 0.26 * size
                };
                painter.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], stroke);
            }
        }
        Icon::Grid => {
            let half = 0.15 * size;
            let gap = 0.36 * size;
            for dx in [-1.0_f32, 1.0] {
                for dy in [-1.0_f32, 1.0] {
                    let cell = Rect::from_center_size(
                        Pos2::new(center.x + dx * gap, center.y + dy * gap),
                        Vec2::new(2.0 * half, 2.0 * half),
                    );
                    painter.rect_stroke(cell, CornerRadius::same(2), stroke, StrokeKind::Inside);
                }
            }
        }
        Icon::Queue => {
            let rows = [-0.27_f32, 0.02];
            for (i, dy) in rows.iter().enumerate() {
                let y = center.y + dy * size;
                let x1 = if i == 0 { 0.34 } else { 0.18 };
                painter.line_segment(
                    [
                        Pos2::new(center.x - 0.34 * size, y),
                        Pos2::new(center.x + x1 * size, y),
                    ],
                    stroke,
                );
            }
            // Nota/ítem ativo na última linha.
            let dot = Pos2::new(center.x + 0.28 * size, center.y + 0.02 * size);
            painter.circle_filled(dot, 0.09 * size, fill);
            let y = center.y + 0.30 * size;
            painter.line_segment(
                [
                    Pos2::new(center.x - 0.34 * size, y),
                    Pos2::new(center.x - 0.02 * size, y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + 0.28 * size, center.y - 0.02 * size),
                    Pos2::new(center.x + 0.28 * size, y),
                ],
                stroke,
            );
        }
        Icon::Volume | Icon::VolumeOff => {
            // Alto-falante: trapézio preenchido à esquerda.
            let base_x = center.x - 0.30 * size;
            let p1 = Pos2::new(base_x, center.y - 0.14 * size);
            let p2 = Pos2::new(base_x, center.y + 0.14 * size);
            let p3 = Pos2::new(center.x - 0.10 * size, center.y + 0.26 * size);
            let p4 = Pos2::new(center.x - 0.10 * size, center.y - 0.26 * size);
            painter.add(Shape::convex_polygon(
                vec![p1, p2, p3, p4],
                fill,
                Stroke::NONE,
            ));
            if matches!(icon, Icon::Volume) {
                for (r, sway) in [(0.18_f32, 0.10), (0.30, 0.16)] {
                    let start = Pos2::new(center.x + 0.02 * size, center.y - sway * size);
                    let control = Pos2::new(center.x + (0.02 + r * 1.2) * size, center.y);
                    let end = Pos2::new(center.x + 0.02 * size, center.y + sway * size);
                    painter.add(Shape::CubicBezier(
                        egui::epaint::CubicBezierShape::from_points_stroke(
                            [start, control, control, end],
                            false,
                            Color32::TRANSPARENT,
                            stroke,
                        ),
                    ));
                }
            } else {
                painter.line_segment(
                    [
                        Pos2::new(center.x + 0.04 * size, center.y - 0.20 * size),
                        Pos2::new(center.x + 0.32 * size, center.y + 0.20 * size),
                    ],
                    stroke,
                );
            }
        }
        Icon::MusicNote => {
            let head = Pos2::new(center.x - 0.08 * size, center.y + 0.24 * size);
            painter.circle_filled(head, 0.14 * size, fill);
            let stem_x = center.x + 0.05 * size;
            painter.line_segment(
                [
                    Pos2::new(stem_x, center.y + 0.24 * size),
                    Pos2::new(stem_x, center.y - 0.28 * size),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(stem_x, center.y - 0.28 * size),
                    Pos2::new(center.x + 0.32 * size, center.y - 0.16 * size),
                ],
                stroke,
            );
        }
        Icon::Warning => {
            let top = Pos2::new(center.x, center.y - 0.32 * size);
            let right = Pos2::new(center.x + 0.36 * size, center.y + 0.26 * size);
            let left = Pos2::new(center.x - 0.36 * size, center.y + 0.26 * size);
            painter.add(Shape::convex_polygon(
                vec![top, right, left],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - 0.10 * size),
                    Pos2::new(center.x, center.y + 0.06 * size),
                ],
                stroke,
            );
            painter.circle_filled(
                Pos2::new(center.x, center.y + 0.19 * size),
                0.035 * size.max(3.0),
                fill,
            );
        }
        Icon::Info => {
            painter.circle_stroke(center, 0.36 * size, stroke);
            painter.circle_filled(
                center - Vec2::new(0.0, 0.16 * size),
                0.045 * size.max(2.5),
                fill,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - 0.02 * size),
                    Pos2::new(center.x, center.y + 0.18 * size),
                ],
                stroke,
            );
        }
    }
}

fn skip_icon(painter: &Painter, center: Pos2, size: f32, fill: Color32, forward: bool) {
    let dir = if forward { 1.0 } else { -1.0 };
    let tip = Pos2::new(center.x + dir * 0.22 * size, center.y);
    let base_top = Pos2::new(center.x - dir * 0.20 * size, center.y - 0.30 * size);
    let base_bottom = Pos2::new(center.x - dir * 0.20 * size, center.y + 0.30 * size);
    painter.add(Shape::convex_polygon(
        vec![tip, base_top, base_bottom],
        fill,
        Stroke::NONE,
    ));
    let bar_x = center.x + dir * 0.32 * size;
    let bar = Rect::from_center_size(
        Pos2::new(bar_x, center.y),
        Vec2::new(0.07 * size, 0.62 * size),
    );
    painter.rect_filled(bar, CornerRadius::same(2), fill);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{FontId, TextWrapMode};

    /// Todos os ícones desenham ao menos uma forma sem pânico.
    #[test]
    fn all_icons_paint_without_panicking() {
        let ctx = egui::Context::default();
        let layer = egui::LayerId::background();
        let painter = egui::Painter::new(
            ctx.clone(),
            layer,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0)),
        );
        let icons = [
            Icon::Play,
            Icon::Pause,
            Icon::SkipBack,
            Icon::SkipForward,
            Icon::Search,
            Icon::ChevronLeft,
            Icon::ChevronRight,
            Icon::ChevronDown,
            Icon::Close,
            Icon::Add,
            Icon::Check,
            Icon::List,
            Icon::Grid,
            Icon::Queue,
            Icon::Volume,
            Icon::VolumeOff,
            Icon::MusicNote,
            Icon::Warning,
            Icon::Info,
        ];
        for icon in icons {
            draw(&painter, icon, Pos2::new(50.0, 50.0), 20.0, Color32::WHITE);
            draw(&painter, icon, Pos2::new(50.0, 50.0), 1.0, Color32::WHITE);
        }
        // Sem pânico em todos os ícones e nos dois extremos de tamanho:
        // formas são registradas no painter (verificação visual fica na galeria).
    }

    /// `Enum` de ícones precisa ser comparável para keys de cache.
    #[test]
    fn icons_are_hashable_and_copiable() {
        let icon = Icon::Play;
        let copy = icon;
        assert_eq!(icon, copy);
        let mut map = std::collections::HashMap::new();
        map.insert(icon, "play");
        assert_eq!(map.get(&Icon::Play), Some(&"play"));
        let _ = FontId::default(); // usa tipos egui para checar versão
        let _ = TextWrapMode::Wrap;
    }
}
