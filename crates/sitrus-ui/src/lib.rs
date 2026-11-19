//! Sitrus-ui: biblioteca de design (tokens, tema, tipografia, componentes).
//!
//! Estrutura:
//! - [`tokens`]: escala de espaçamento, raios, alturas e escada tipográfica.
//! - [`theme`]: papéis semânticos de cor (escuro padrão + claro, acento configurável).
//! - [`typography`]: Inter variável embutida (OFL 1.1) e `FontId` por token.
//! - [`anim`]: ease-out cúbica e durações 120/180/240 ms.
//! - [`artwork`]: decodificação, cores dominantes, gradiente de hero e blur em software.
//! - [`components`]: catálogo com todos os estados (normal/hover/pressed/focus/disabled/selecionado).
//! - [`gallery`]: tela obrigatória de desenvolvimento (`gallery(ui)`).

pub mod anim;
pub mod artwork;
pub mod components;
pub mod format;
pub mod gallery;
pub mod icons;
pub mod painting;
pub mod theme;
pub mod tokens;
pub mod typography;

/// Reexport da função da galeria para o app chamar em `--gallery`.
pub use gallery::gallery;
/// Tema principal, reexportado para o app aplicar na janela inteira.
pub use theme::Theme;

use eframe::App;
use sitrus_core::model::Track;

/// Telas previstas pelo design system; cada uma vira um módulo próprio depois.
const GALLERY_ENTRIES: &[&str] = &["Home", "Search", "Now Playing", "Library"];

/// Janela principal do Sitrus; implementa o ciclo de frames do eframe.
#[derive(Debug)]
pub struct SitrusApp {
    /// Cabeçalho exibido no topo da janela (placeholder até as telas existirem).
    pub heading: String,
    /// Faixa em reprodução, usada pela tela de "Now Playing" quando implementada.
    pub now_playing: Option<Track>,
    /// `true` renderiza a galeria de componentes (modo `--gallery`).
    pub gallery_mode: bool,
}

impl SitrusApp {
    /// Cria a janela com o cabeçalho padrão do produto.
    #[must_use]
    pub fn new() -> Self {
        Self {
            heading: "Sitrus".to_owned(),
            now_playing: None,
            gallery_mode: false,
        }
    }

    /// Configura o app para renderizar a galeria (flag `--gallery`).
    #[must_use]
    pub fn with_gallery(mut self) -> Self {
        self.gallery_mode = true;
        self
    }

    /// Lista de telas registradas no esqueleto, exposta para o binário.
    #[must_use]
    pub fn gallery_entries() -> &'static [&'static str] {
        GALLERY_ENTRIES
    }

    /// Tema corrente da janela (escuro por padrão; claro via `with_accent`/paleta).
    #[must_use]
    pub fn theme(&self) -> Theme {
        Theme::dark()
    }
}

impl Default for SitrusApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for SitrusApp {
    /// Desenha um frame; a galeria cobre todos os componentes e estados.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        typography::install_once(&ctx);
        let theme = self.theme();
        theme.apply(&ctx);

        // O `Ui` vindo do eframe não tem margem nem fundo; o CentralPanel
        // devolve a aparência padrão do app.
        egui::CentralPanel::default().show(ui, |ui| {
            if self.gallery_mode {
                gallery::gallery(ui);
            } else {
                ui.scope(|ui| {
                    theme.apply_ui(ui);
                    ui.heading(
                        egui::RichText::new(&self.heading)
                            .font(typography::font_id(tokens::TypeToken::LargeTitle)),
                    );
                    if let Some(track) = &self.now_playing {
                        ui.label(
                            egui::RichText::new(format!(
                                "Tocando agora: {}",
                                track.display_title()
                            ))
                            .color(theme.label_secondary),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(
                                "Telas Home/Search/Now Playing/Library entram nas próximas \
                                 iterações; use --gallery para revisar o design system.",
                            )
                            .color(theme.label_secondary),
                        );
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A galeria registra as quatro telas do design system.
    #[test]
    fn gallery_lists_design_system_screens() {
        let entries = SitrusApp::gallery_entries();
        assert_eq!(entries.len(), 4);
        assert!(entries.contains(&"Now Playing"));
    }

    /// O cabeçalho padrão é o nome do produto e o modo galeria é opt-in.
    #[test]
    fn default_heading_and_gallery_flag() {
        let app = SitrusApp::new();
        assert_eq!(app.heading, "Sitrus");
        assert!(app.now_playing.is_none());
        assert!(!app.gallery_mode);
        assert!(SitrusApp::new().with_gallery().gallery_mode);
        assert!(SitrusApp::default().theme().dark_mode);
    }
}
