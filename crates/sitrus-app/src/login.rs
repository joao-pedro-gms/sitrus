//! Tela de login: caminho manual de colar cookies (sempre disponível) e,
//! com a feature `webview`, botão para abrir o player web em janela própria.
//!
//! Nenhum valor de cookie/token é exibido além do sinal de presença nem
//! registrado em log.

use egui::{Id, TextEdit, Ui};
use sitrus_api::Session;
use sitrus_ui::components::button::{self, ButtonContent, ButtonVariant};
use sitrus_ui::components::error_banner::{self, BannerKind};
use sitrus_ui::theme::Theme;
use sitrus_ui::tokens::{self, type_scale};
use sitrus_ui::typography;

/// Ação pedida pela tela de login no frame corrente.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum LoginAction {
    /// Nada pedido.
    #[default]
    None,
    /// Salvar a sessão a partir do texto colado.
    Save(String),
    /// Abrir a janela de webview (feature `webview`).
    OpenWebview,
    /// Encerrar a sessão do dispositivo.
    Logout,
}

/// Estado local da tela de login.
#[derive(Default, Debug)]
pub struct LoginUi {
    /// Texto colado pelo usuário (header `Cookie:` ou pares nome=valor).
    pub paste: String,
    /// Janela de login por webview está aberta (feature `webview`).
    pub webview_open: bool,
    /// Mensagem de status para a tela (erro/aviso), sem credenciais.
    pub message: Option<(BannerKind, String)>,
}

impl LoginUi {
    /// Limpa a área de colagem e a mensagem (após entrar com sucesso).
    pub fn reset(&mut self) {
        self.paste = String::new();
        self.message = None;
        self.webview_open = false;
    }
}

/// Desenha a tela de login dentro da área central e devolve a ação pedida.
pub fn render_login_screen(
    ui: &mut Ui,
    theme: &Theme,
    login: &mut LoginUi,
    has_session: bool,
) -> LoginAction {
    let mut action = LoginAction::None;
    ui.vertical(|page| {
        page.add_space(tokens::spacing::X40);
        padded(page, |column| {
            column.label(
                egui::RichText::new("Entrar na Apple Music")
                    .font(typography::font_id(type_scale::TypeToken::LargeTitle))
                    .color(theme.label_primary),
            );
            column.add_space(tokens::spacing::X8);
            column.label(
                egui::RichText::new(
                    "O Sitrus usa a sua sessão do player web da Apple Music \
                     (media-user-token) para ler a sua biblioteca. Entre no \
                     navegador ou cole os cookies manualmente — nenhum valor \
                     é exibido ou registrado.",
                )
                .font(typography::font_id(type_scale::TypeToken::Subheadline))
                .color(theme.label_secondary),
            );
            column.add_space(tokens::spacing::X24);

            if has_session {
                column.label(
                    egui::RichText::new("Sessão ativa neste dispositivo.")
                        .font(typography::font_id(type_scale::TypeToken::Footnote))
                        .color(theme.success),
                );
                column.add_space(tokens::spacing::X12);
            } else if let Some((kind, message)) = &login.message {
                let close = error_banner::error_banner(
                    column,
                    theme,
                    Id::unique("sitrus_login_message"),
                    *kind,
                    message,
                    true,
                );
                if let Some(button) = close
                    && button.clicked()
                {
                    login.message = None;
                }
                column.add_space(tokens::spacing::X12);
            }

            // Caminho por webview: o botão existe nas duas builds; sem a
            // feature `webview`, o app responde com aviso interativo.
            {
                if button::button(
                    column,
                    theme,
                    Id::unique("sitrus_login_webview"),
                    ButtonVariant::Primary,
                    ButtonContent::label("Entrar no navegador"),
                    true,
                    false,
                )
                .clicked()
                {
                    action = LoginAction::OpenWebview;
                }
                #[cfg(feature = "webview")]
                if login.webview_open {
                    column.add_space(tokens::spacing::X8);
                    column.label(
                        egui::RichText::new(
                            "Janela de login aberta: entre na sua conta; a \
                             sessão é capturada quando o player conclui o \
                             acesso.",
                        )
                        .font(typography::font_id(type_scale::TypeToken::Footnote))
                        .color(theme.label_secondary),
                    );
                }
                column.add_space(tokens::spacing::X12);
                column.label(
                    egui::RichText::new("ou cole os cookies manualmente")
                        .font(typography::font_id(type_scale::TypeToken::Footnote))
                        .color(theme.label_tertiary),
                );
            }

            column.add_space(tokens::spacing::X12);
            column.label(
                egui::RichText::new("Cookies do player web (manual)")
                    .font(typography::font_id(type_scale::TypeToken::Title2))
                    .color(theme.label_primary),
            );
            column.add_space(tokens::spacing::X8);
            column.label(
                egui::RichText::new(
                    "No navegador, com o player aberto: DevTools → Rede → \
                     pedidos a amp-api.music.apple.com → copie o cabeçalho \
                     `Cookie:` inteiro e cole abaixo.",
                )
                .font(typography::font_id(type_scale::TypeToken::Footnote))
                .color(theme.label_secondary),
            );
            column.add_space(tokens::spacing::X8);
            TextEdit::multiline(&mut login.paste)
                .id(Id::unique("sitrus_login_paste"))
                .desired_width(column.available_width())
                .desired_rows(6)
                .hint_text("Cookie: media-user-token=…; geo=br; …")
                .font(egui::FontSelection::FontId(typography::font_id(
                    type_scale::TypeToken::Footnote,
                )))
                .text_color(theme.label_primary)
                .show(column);
            column.add_space(tokens::spacing::X12);

            if button::button(
                column,
                theme,
                Id::unique("sitrus_login_save"),
                ButtonVariant::Secondary,
                ButtonContent::label("Salvar sessão"),
                true,
                false,
            )
            .clicked()
            {
                action = LoginAction::Save(login.paste.clone());
            }

            if has_session {
                column.add_space(tokens::spacing::X8);
                if button::button(
                    column,
                    theme,
                    Id::unique("sitrus_login_logout"),
                    ButtonVariant::Plain,
                    ButtonContent::label("Sair do dispositivo"),
                    true,
                    false,
                )
                .clicked()
                {
                    action = LoginAction::Logout;
                }
            }
        });
    });
    action
}

/// Corpo com margem lateral de 24 px (retângulo exato, cursor avançado).
fn padded(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    let inner = ui
        .available_rect_before_wrap()
        .shrink2(egui::vec2(tokens::spacing::X24, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        body,
    );
}

/// Converte o texto colado numa `Session` e valida a presença do token.
///
/// Devolve `Err` com mensagem de UI quando o media-user-token não aparece.
pub fn parse_pasted_session(pasted: &str) -> Result<Session, String> {
    let session = Session::from_pasted_cookies(pasted);
    if session.media_user_token.is_none() {
        return Err("Os cookies colados não contêm o media-user-token. Copie o \
             cabeçalho `Cookie:` de um pedido ao amp-api com o player aberto."
            .to_owned());
    }
    Ok(session)
}
