//! Cabeçalho de página de detalhe: linha de voltar (chevron + "Voltar")
//! sobreposta ao HeroHeader compartilhado (capa, título, subtítulo, tocar).
//!
//! Devolve as respostas dos dois gestos — voltar e tocar — para a tela
//! conectar à pilha de navegação e à fila de reprodução.

use egui::{Id, Response, TextureHandle, Ui};

use crate::components::button::{self, ButtonContent, ButtonVariant};
use crate::components::hero_header::{self, HeroData};
use crate::icons::Icon;
use crate::theme::Theme;
use crate::tokens::spacing;

/// Dados do cabeçalho de detalhe.
#[derive(Clone, Debug)]
pub struct DetailHeader<'a> {
    /// Hero (título, subtítulo, rótulo e gradiente).
    pub hero: HeroData<'a>,
    /// Botão de tocar habitado? (Páginas sem faixas desabilitam.)
    pub play_enabled: bool,
    /// Botão de voltar habitado? (Raiz da pilha desabilita.)
    pub back_enabled: bool,
    /// Rótulo do botão de voltar (padrão "Voltar").
    pub back_label: &'a str,
}

impl<'a> DetailHeader<'a> {
    /// Cabeçalho padrão: voltar e tocar habilitados, rótulo "Voltar".
    #[must_use]
    pub fn new(hero: HeroData<'a>) -> Self {
        Self {
            hero,
            play_enabled: true,
            back_enabled: true,
            back_label: "Voltar",
        }
    }
}

/// Respostas do cabeçalho (`back` e `play` podem ser nulos quando
/// desabilitados; `.clicked()` é falso e seguro de consultar).
#[derive(Debug)]
pub struct DetailHeaderOutput {
    /// Resposta do botão de voltar.
    pub back: Response,
    /// Resposta do botão de tocar do hero.
    pub play: Response,
}

/// Desenha a linha de voltar + hero; devolve os gestos correspondentes.
pub fn detail_header(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    header: &DetailHeader<'_>,
    artwork: Option<&TextureHandle>,
) -> DetailHeaderOutput {
    // Linha de voltar: respiro do padding lateral do conteúdo (12 px),
    // botão secundário compacto com chevron + rótulo.
    let back = ui
        .horizontal_top(|row| {
            row.add_space(spacing::X12);
            button::button(
                row,
                theme,
                id.with("back"),
                ButtonVariant::Secondary,
                ButtonContent::icon_and_text(Icon::ChevronLeft, header.back_label),
                header.back_enabled,
                false,
            )
        })
        .inner;
    ui.add_space(spacing::X8);
    let play = hero_header::hero_header(
        ui,
        theme,
        id.with("hero"),
        artwork,
        &header.hero,
        header.play_enabled,
    );
    DetailHeaderOutput { back, play }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O cabeçalho padrão habilita voltar e tocar com rótulo "Voltar".
    #[test]
    fn defaults_enable_back_and_play() {
        let header = DetailHeader::new(HeroData::new("Álbum", None));
        assert!(header.back_enabled);
        assert!(header.play_enabled);
        assert_eq!(header.back_label, "Voltar");
    }

    /// O hero do cabeçalho é reutilizável: título segue os dados passados.
    #[test]
    fn hero_is_passed_through() {
        let hero = HeroData {
            title: "Playlist",
            subtitle: Some("Curadoria"),
            action_label: Some("Tocar tudo"),
            gradient: None,
        };
        let header = DetailHeader::new(hero);
        assert_eq!(header.hero.title, "Playlist");
        assert_eq!(header.hero.action_label, Some("Tocar tudo"));
    }
}
