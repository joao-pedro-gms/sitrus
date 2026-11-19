//! Primitivas de pintura compartilhadas (gradientes por mesh, véus).

use egui::{Color32, Mesh, Painter, Pos2, Rect, Vec2, epaint::Vertex};

/// Pinta um gradiente linear de `left` → `right` (4 vértices, 2 triângulos).
pub fn gradient_horizontal(painter: &Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    push_quad(&mut mesh, rect, left, right, right, left);
    painter.add(egui::Shape::mesh(mesh));
}

/// Pinta um gradiente vertical `top` → `bottom`.
pub fn gradient_vertical(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    push_quad(&mut mesh, rect, top, top, bottom, bottom);
    painter.add(egui::Shape::mesh(mesh));
}

/// Quad com cores por canto: (NW, NE, SW, SE).
pub fn push_quad(mesh: &mut Mesh, rect: Rect, nw: Color32, ne: Color32, sw: Color32, se: Color32) {
    let uv = egui::epaint::WHITE_UV;
    let v0 = mesh.vertices.len() as u32;
    let verts = [
        (Pos2::new(rect.left(), rect.top()), nw),
        (Pos2::new(rect.right(), rect.top()), ne),
        (Pos2::new(rect.left(), rect.bottom()), sw),
        (Pos2::new(rect.right(), rect.bottom()), se),
    ];
    for (pos, color) in verts {
        mesh.vertices.push(Vertex { pos, uv, color });
    }
    mesh.add_triangle(v0, v0 + 2, v0 + 3);
    mesh.add_triangle(v0, v0 + 3, v0 + 1);
}

/// "Véu" escuro para contraste do texto branco sobre o hero: alpha baixo no
/// topo, mais forte na base (design: hero é o caso crítico de contraste).
pub fn dark_veil(painter: &Painter, rect: Rect) {
    let top = Color32::from_black_alpha(70);
    let bottom = Color32::from_black_alpha(120);
    gradient_vertical(painter, rect, top, bottom);
}

/// Gradiente "placeholder" para artwork ausente.
pub fn placeholder_art(painter: &Painter, rect: Rect, from: Color32, to: Color32) {
    gradient_horizontal(painter, rect, from, to);
}

#[allow(unused)]
pub(crate) fn unit_vec() -> Vec2 {
    Vec2::ONE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quad de gradiente gera 4 vértices e 2 triângulos em ordem correta.
    #[test]
    fn gradient_mesh_has_four_vertices_six_indices() {
        let mut mesh = Mesh::default();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0));
        push_quad(
            &mut mesh,
            rect,
            Color32::RED,
            Color32::GREEN,
            Color32::BLUE,
            Color32::WHITE,
        );
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.vertices[0].color, Color32::RED);
        assert_eq!(mesh.vertices[1].color, Color32::GREEN);
        assert_eq!(mesh.vertices[2].color, Color32::BLUE);
        assert_eq!(mesh.vertices[3].color, Color32::WHITE);
        // Triângulos cobrem os 4 vértices (indices 0..3).
        assert!(mesh.indices.iter().all(|i| *i < 4));
    }

    /// O véu pinta dois meshes (gradiente) sem pânico no painter real.
    #[test]
    fn veil_paints_translucent_black_meshes() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(10.0));
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), rect);
        dark_veil(&painter, rect);
        gradient_vertical(&painter, rect, Color32::RED, Color32::BLUE);
        gradient_horizontal(&painter, rect, Color32::BLACK, Color32::WHITE);
    }
}
