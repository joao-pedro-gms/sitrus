//! Tabela de faixas de página de detalhe: colunas número/título/duração com
//! cabeçalho e linhas densas de 44 px (reutiliza [`crate::components::list_row`]).
//!
//! O clique em qualquer linha é o gesto "tocar a partir daqui": devolve o
//! índice clicado para a tela montar a fila a partir dele.

use egui::{Align2, Id, Sense, Ui, Vec2};

use crate::components::list_row::{ListRow, song_row};
use crate::theme::Theme;
use crate::tokens::{spacing, type_scale};
use crate::typography;

/// Altura do cabeçalho de colunas.
pub const HEADER_HEIGHT: f32 = 20.0;

/// Rótulos das colunas do cabeçalho, na ordem número/título/duração.
pub const COLUMN_HEADERS: [&str; 3] = ["#", "Título", "Duração"];

/// Linha de faixa da tabela (dados apenas, sem contexto gráfico).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackRow<'a> {
    /// Índice 0-based da faixa (desenhado como número 1-based no slot).
    pub index: Option<usize>,
    /// Título.
    pub title: &'a str,
    /// Subtítulo (artista ou álbum, conforme a página).
    pub subtitle: Option<&'a str>,
    /// Duração em segundos (`None`/`Some(0)` = desconhecida, sem rótulo).
    pub duration_secs: Option<u32>,
}

impl<'a> TrackRow<'a> {
    /// Linha numerada com título, subtítulo e duração.
    #[must_use]
    pub fn new(
        index: usize,
        title: &'a str,
        subtitle: Option<&'a str>,
        duration_secs: Option<u32>,
    ) -> Self {
        Self {
            index: Some(index),
            title,
            subtitle,
            duration_secs,
        }
    }

    /// Duração exibível: zero é desconhecida e não ganha rótulo.
    #[must_use]
    pub const fn display_duration(&self) -> Option<u32> {
        match self.duration_secs {
            Some(secs) if secs > 0 => Some(secs),
            _ => None,
        }
    }
}

/// Desenha a tabela completa (cabeçalho + linhas) e devolve o índice
/// clicado, se houver.
///
/// O parâmetro `selected_index` destaca a faixa em reprodução quando a fila
/// foi montada a partir desta mesma coleção; páginas sem fila ativa passam
/// `None`.
pub fn track_table(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    rows: &[TrackRow<'_>],
    selected_index: Option<usize>,
) -> Option<usize> {
    // Cabeçalho de colunas alinhado às âncoras internas das linhas.
    let width = ui.available_width();
    let (header, _) = ui.allocate_exact_size(Vec2::new(width, HEADER_HEIGHT), Sense::hover());
    let painter = ui.painter();
    let header_color = theme.label_tertiary;
    let font = typography::font_id(type_scale::TypeToken::Footnote);
    painter.text(
        egui::Pos2::new(header.left() + spacing::X12 + 16.0, header.center().y),
        Align2::CENTER_CENTER,
        COLUMN_HEADERS[0],
        font.clone(),
        header_color,
    );
    painter.text(
        egui::Pos2::new(
            header.left() + spacing::X12 + 32.0 + spacing::X12,
            header.center().y,
        ),
        Align2::LEFT_CENTER,
        COLUMN_HEADERS[1],
        font.clone(),
        header_color,
    );
    painter.text(
        egui::Pos2::new(
            // Anchor central da coluna de duração/hover das linhas.
            header.right() - spacing::X24,
            header.center().y,
        ),
        Align2::RIGHT_CENTER,
        COLUMN_HEADERS[2],
        font.clone(),
        header_color,
    );

    let mut selected = None;
    for (i, row) in rows.iter().enumerate() {
        let response = song_row(
            ui,
            theme,
            id.with(("track", i)),
            ListRow {
                index: row.index,
                title: row.title,
                subtitle: row.subtitle,
                duration_secs: row.display_duration(),
                artwork: None,
                hover_icon: None,
                hover_tooltip: None,
            },
            true,
            selected_index == Some(i),
        );
        if response.clicked() && selected.is_none() {
            selected = Some(i);
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rótulos de coluna fixos e ordenados.
    #[test]
    fn column_headers_are_stable() {
        assert_eq!(COLUMN_HEADERS, ["#", "Título", "Duração"]);
    }

    /// Duração zero (desconhecida) não ganha rótulo; valores conhecidos ficam.
    #[test]
    fn zero_duration_maps_to_none() {
        let hidden = TrackRow::new(0, "Faixa", None, Some(0));
        assert_eq!(hidden.display_duration(), None);
        let visible = TrackRow::new(1, "Faixa", Some("Artista"), Some(83));
        assert_eq!(visible.display_duration(), Some(83));
        let absent = TrackRow::new(2, "Faixa", None, None);
        assert_eq!(absent.display_duration(), None);
    }

    /// Linhas nascem numeradas; o ListRow compartilhado exibe índice + 1.
    #[test]
    fn rows_are_one_based_on_display() {
        let row = TrackRow::new(0, "T", None, None);
        assert_eq!(row.index, Some(0));
        let list = ListRow {
            index: row.index,
            ..ListRow::plain(row.title)
        };
        assert_eq!(list.index.map(|i| i + 1), Some(1));
    }
}
