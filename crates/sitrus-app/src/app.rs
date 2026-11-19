//! Estado do aplicativo e janela principal (eframe).
//!
//! Estrutura da janela: rail de navegação de 72 px (esquerda), área central
//! por tela e barra de player fixa no rodapé (88 px). Todo I/O de rede fica
//! na thread de [`crate::net`] e chega por canal; eventos do backend de
//! áudio (`sitrus-player`) são drenados a cada frame. Nada bloqueia o frame.
//!
//! Tema: escuro por padrão; quando a configuração pede `System` e o sistema
//! expõe um tema (Wayland/KDE), o app acompanha o tema do sistema.
//!
//! Nenhum valor de cookie/token é registrado ou exibido — a sessão é
//! sinalizada apenas por presença.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use egui::{Color32, Context, Id, ScrollArea, TextureHandle, Vec2, ViewportCommand};
use sitrus_api::Session;
use sitrus_core::Settings;
use sitrus_core::config::Theme as CoreTheme;
use sitrus_ui::components::error_banner::BannerKind;
use sitrus_ui::icons::Icon;
use sitrus_ui::tokens::{spacing, type_scale};
use sitrus_ui::typography;
use sitrus_ui::{components as comps, theme::Theme};

use crate::login::{self, LoginAction, LoginUi};
use crate::model::{ListMode, PlaylistRow, SectionState, SongRow};
use crate::net::{NetCommand, NetEvent, spawn as spawn_net};
use crate::player::PlayerController;

/// Tamanho inicial da janela (design).
pub const WINDOW_SIZE: Vec2 = Vec2::new(1280.0, 820.0);
/// Tamanho mínimo da janela.
pub const MIN_WINDOW_SIZE: Vec2 = Vec2::new(980.0, 620.0);
/// Título da janela.
pub const WINDOW_TITLE: &str = "Sitrus";
/// Quantidade de itens por prateleira da Home.
const HOME_SHELF_ITEMS: usize = 12;
/// Linhas de skeleton por tela em carregamento.
const SKELETON_ROWS: usize = 8;
/// Colunas de margem do conteúdo (design: 24 px das bordas da janela).
const CONTENT_X24: f32 = spacing::X24;

/// Id do campo de busca (raiz do `search_field`).
fn search_field_id() -> Id {
    Id::unique("sitrus_search_field")
}

/// Id do widget de edição dentro do campo de busca (convenção do componente:
/// `field_id.with("edit_widget")`) — foco via Ctrl+F e Enter apontam aqui.
fn search_edit_id() -> Id {
    search_field_id().with("edit_widget")
}

/// Id do toast compartilhado.
fn toast_id() -> Id {
    Id::unique("sitrus_toast")
}

/// Corpo com margem lateral de 24 px (retângulo exato, cursor avançado).
fn padded(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    let inner = ui
        .available_rect_before_wrap()
        .shrink2(egui::vec2(CONTENT_X24, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        body,
    );
}

/// Tela ativa no rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Início (hero + prateleiras).
    Home,
    /// Biblioteca (faixas).
    Library,
    /// Playlists do usuário.
    Playlists,
    /// Busca (catálogo + fallback local).
    Search,
    /// Entrar (login manual/webview).
    Login,
}

impl Screen {
    /// Índice no rail; Login não aparece no rail.
    #[must_use]
    pub const fn rail_index(self) -> Option<usize> {
        match self {
            Self::Home => Some(0),
            Self::Library => Some(1),
            Self::Playlists => Some(2),
            Self::Search => Some(3),
            Self::Login => None,
        }
    }
}

/// Tela de busca (termo corrente, enviado e resultados).
#[derive(Debug, Default)]
struct SearchState {
    /// Texto digitado.
    input: String,
    /// Último termo enviado (dedupe de Enter repetido).
    submitted: String,
    /// Resultados correntes.
    results: Vec<SongRow>,
    /// Estado da busca.
    state: SectionState,
    /// Origem dos resultados ("Catálogo…" / "Sua biblioteca").
    source: &'static str,
    /// Nota de fallback, quando a busca caiu na biblioteca local.
    note: Option<String>,
}

/// Aplicativo principal; implementa `eframe::App`.
pub struct App {
    /// Preferências persistentes (tema, volume, modo lista automático).
    settings: Settings,
    /// Tela ativa.
    screen: Screen,
    /// Estado local da tela de login.
    login: LoginUi,
    /// Sessão ativa (presença; valores nunca são exibidos ou logados).
    session: Option<Session>,
    /// Comandos para o worker de rede.
    net_tx: Sender<NetCommand>,
    /// Eventos do worker de rede (e do subprocesso de login).
    net_rx: Receiver<NetEvent>,
    /// Sender de eventos para produtores fora do worker: o watcher do
    /// subprocesso de login por webview publica em `net_rx` a partir dele.
    #[cfg(feature = "webview")]
    login_event_tx: Sender<NetEvent>,
    /// Faixas da biblioteca.
    songs: Vec<SongRow>,
    /// A biblioteca coube inteira dentro dos limiares de paginação.
    songs_complete: bool,
    /// Estado da seção de faixas.
    songs_state: SectionState,
    /// Playlists da biblioteca.
    playlists: Vec<PlaylistRow>,
    /// As playlists couberam inteira dentro dos limiares.
    playlists_complete: bool,
    /// Estado da seção de playlists.
    playlists_state: SectionState,
    /// Modo de apresentação da biblioteca (prateleiras/lista).
    library_mode: ListMode,
    /// Modo de apresentação das playlists.
    playlists_mode: ListMode,
    /// Estado da busca.
    search: SearchState,
    /// Texturas de artwork por chave (id do item).
    artworks: HashMap<String, TextureHandle>,
    /// Gradientes derivados por chave (hero).
    gradients: HashMap<String, (Color32, Color32)>,
    /// Capas pedidas e ainda não resolvidas.
    pending_artwork: HashSet<String>,
    /// Player + fila de reprodução (`sitrus-player` + `sitrus-core`).
    player: PlayerController,
    /// Painel de fila visível?
    queue_open: bool,
    /// Toast temporário.
    toast: Option<(BannerKind, String)>,
    /// O toast precisa ser reativado no próximo frame.
    toast_fresh: bool,
}

impl App {
    /// Cria o aplicativo; `manual_login` abre direto na tela de login.
    #[must_use]
    pub fn new(ctx: &Context, manual_login: bool) -> Self {
        let settings = Settings::load_default().unwrap_or_default();
        if let Some(zoom) = settings.ui_scale {
            ctx.set_zoom_factor(zoom.clamp(0.5, 3.0));
        }
        let (net_tx, net_rx, _login_event_tx) = spawn_net();

        let mut app = Self {
            settings,
            screen: Screen::Home,
            login: LoginUi::default(),
            session: None,
            net_tx,
            net_rx,
            #[cfg(feature = "webview")]
            login_event_tx: _login_event_tx,
            songs: Vec::new(),
            songs_complete: true,
            songs_state: SectionState::Loaded,
            playlists: Vec::new(),
            playlists_complete: true,
            playlists_state: SectionState::Loaded,
            library_mode: ListMode::Shelves,
            playlists_mode: ListMode::Shelves,
            search: SearchState::default(),
            artworks: HashMap::new(),
            gradients: HashMap::new(),
            pending_artwork: HashSet::new(),
            player: PlayerController::new(),
            queue_open: false,
            toast: None,
            toast_fresh: false,
        };
        app.player.set_volume(app.settings.volume.clamp(0.0, 1.0));

        // Sessão restaurada do disco: abre com as telas carregando.
        match Session::load() {
            Ok(Some(session)) if session.media_user_token.is_some() => {
                app.session = Some(session.clone());
                app.set_library_loading();
                let _ = app.net_tx.send(NetCommand::SetSession(session));
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(_) => {
                app.set_toast(
                    BannerKind::Warning,
                    "Não foi possível ler a sessão salva no disco.".to_owned(),
                );
            }
        }
        if manual_login || app.session.is_none() {
            app.screen = Screen::Login;
        }
        app
    }

    /// Tema efetivo: escuro por padrão; acompanha o sistema quando a
    /// configuração pede e o sistema expõe um tema.
    fn current_theme(&self, ctx: &Context) -> Theme {
        match self.settings.theme {
            CoreTheme::System => match ctx.system_theme() {
                Some(egui::Theme::Light) => Theme::light(),
                Some(egui::Theme::Dark) | None => Theme::dark(),
            },
            CoreTheme::Light => Theme::light(),
            CoreTheme::Dark => Theme::dark(),
        }
    }

    /// Há sessão utilizável?
    fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Toast temporário.
    fn set_toast(&mut self, kind: BannerKind, message: String) {
        self.toast = Some((kind, message));
        self.toast_fresh = true;
    }

    /// Seções como em carregamento (antes/esperando o worker).
    fn set_library_loading(&mut self) {
        self.songs_state = SectionState::Loading;
        self.playlists_state = SectionState::Loading;
    }

    /// Instala uma sessão recém-criada (manual ou webview).
    fn apply_session(&mut self, session: Session) {
        self.adopt_session(&session);
        let _ = self.net_tx.send(NetCommand::SetSession(session));
    }

    /// Aplica à UI a presença de uma sessão recém-instalada no worker.
    ///
    /// O comando `SetSession` correspondente pode partir daqui (login manual)
    /// ou do watcher do login por webview, que entrega a sessão relida do
    /// disco antes de o worker publicar `SessionReady`.
    fn adopt_session(&mut self, session: &Session) {
        self.session = Some(session.clone());
        self.songs.clear();
        self.playlists.clear();
        self.set_library_loading();
        self.screen = Screen::Home;
        self.login.reset();
        self.set_toast(
            BannerKind::Info,
            "Sessão carregada: biblioteca em preparo…".to_owned(),
        );
    }

    /// Login manual: valida o texto colado e instala a sessão.
    fn on_manual_login(&mut self, pasted: &str) {
        match login::parse_pasted_session(pasted) {
            Ok(session) => self.apply_session(session),
            Err(message) => {
                self.login.message = Some((BannerKind::Error, message));
            }
        }
    }

    /// Logout: descarta sessão do app e do disco.
    fn logout(&mut self) {
        self.session = None;
        self.screen = Screen::Login;
        let _ = self.net_tx.send(NetCommand::ClearSession);
        if let Ok(path) = Session::session_path() {
            let _ = std::fs::remove_file(path);
        }
        self.songs.clear();
        self.playlists.clear();
        self.songs_state = SectionState::Loaded;
        self.playlists_state = SectionState::Loaded;
        self.search = SearchState::default();
        self.player.clear();
        self.set_toast(
            BannerKind::Info,
            "Sessão encerrada neste dispositivo.".to_owned(),
        );
    }

    /// Recarrega a biblioteca (ação de "Tentar de novo").
    fn refresh_library(&mut self) {
        self.set_library_loading();
        if let Some(session) = self.session.clone() {
            let _ = self.net_tx.send(NetCommand::SetSession(session));
        }
    }

    /// Login via webview (subprocesso); somente com a feature `webview`.
    fn open_webview_login(&mut self) {
        self.login.message = None;
        #[cfg(feature = "webview")]
        if !self.login.webview_open {
            crate::webview::spawn_login_process(self.net_tx.clone(), self.login_event_tx.clone());
            self.login.webview_open = true;
        }
        #[cfg(not(feature = "webview"))]
        {
            self.login.message = Some((
                BannerKind::Warning,
                "Esta versão foi compilada sem o login por navegador \
                 (feature `webview`); use o caminho manual de cookies."
                    .to_owned(),
            ));
        }
    }

    /// Envia uma busca ao worker quando o termo muda.
    fn submit_search(&mut self) {
        let term = self.search.input.trim().to_owned();
        if term.is_empty() || self.search.submitted == term {
            return;
        }
        self.search.submitted = term.clone();
        self.search.results.clear();
        self.search.state = SectionState::Loading;
        self.search.source = "";
        self.search.note = None;
        let _ = self.net_tx.send(NetCommand::Search(term));
    }

    /// Processa um evento do worker.
    fn apply_net_event(&mut self, event: NetEvent, ctx: &Context) {
        match event {
            NetEvent::SessionReady => {
                // O worker instalou uma sessão que a UI não conhece: foi o
                // watcher do subprocesso de login por webview relendo
                // `Session::load()` do disco e entregando a sessão como
                // `SetSession` (o subprocesso gravou antes de emitir o
                // marcador). Nos demais caminhos (restauro, manual, recarga)
                // o `SetSession` parte da própria UI e `self.session` já
                // reflete a sessão — nada muda.
                if self.login.webview_open
                    && let Ok(Some(session)) = Session::load()
                    && session.media_user_token.is_some()
                {
                    self.adopt_session(&session);
                }
            }
            NetEvent::LoginError(message) => {
                self.login.message = Some((BannerKind::Error, message));
                self.login.webview_open = false;
            }
            NetEvent::Notice(message) if !message.is_empty() => {
                self.set_toast(BannerKind::Info, message);
            }
            NetEvent::Notice(_) => {}
            NetEvent::LibrarySongs { rows, complete } => {
                self.songs = rows;
                self.songs_complete = complete;
                self.songs_state = SectionState::Loaded;
            }
            NetEvent::LibrarySongsFailed(message) => {
                self.songs_state = SectionState::Failed(message);
            }
            NetEvent::LibraryPlaylists { rows, complete } => {
                self.playlists = rows;
                self.playlists_complete = complete;
                self.playlists_state = SectionState::Loaded;
            }
            NetEvent::LibraryPlaylistsFailed(message) => {
                self.playlists_state = SectionState::Failed(message);
            }
            NetEvent::SearchDone {
                term,
                rows,
                source,
                note,
            } => {
                if term == self.search.submitted {
                    self.search.results = rows;
                    self.search.state = SectionState::Loaded;
                    self.search.source = source;
                    self.search.note = note;
                }
            }
            NetEvent::SearchFailed { term, message } => {
                if term == self.search.submitted {
                    self.search.state = SectionState::Failed(message);
                }
            }
            NetEvent::ArtworkDone {
                key,
                image,
                gradient,
            } => {
                self.pending_artwork.remove(&key);
                if let Some(image) = image {
                    let name = format!("sitrus-artwork:{key}");
                    let handle = sitrus_ui::artwork::load_texture(ctx, &name, &image);
                    self.artworks.insert(key.clone(), handle);
                }
                if let Some((from, to)) = gradient {
                    self.gradients.insert(key, (from, to));
                }
            }
        }
    }

    /// Drena todos os eventos pendentes (rede e áudio).
    fn drain_events(&mut self, ctx: &Context) {
        loop {
            match self.net_rx.try_recv() {
                Ok(event) => self.apply_net_event(event, ctx),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        let mut player_errors = Vec::new();
        if self.player.drain_player_events(&mut player_errors) {
            // Faixa terminou: avança na fila (para no fim).
            self.player.advance();
        }
        for message in player_errors {
            self.set_toast(BannerKind::Error, format!("Reprodução: {message}"));
        }
    }

    /// Pede uma capa ao worker (ignora repetições). `true` quando pedido.
    fn request_artwork(&mut self, key: &str, url: Option<&str>) -> bool {
        if self.artworks.contains_key(key) || self.pending_artwork.contains(key) {
            return false;
        }
        let Some(url) = url.filter(|candidate| !candidate.is_empty()) else {
            return false;
        };
        self.pending_artwork.insert(key.to_owned());
        if self
            .net_tx
            .send(NetCommand::Artwork {
                key: key.to_owned(),
                url: url.to_owned(),
            })
            .is_err()
        {
            self.pending_artwork.remove(key);
            return false;
        }
        true
    }

    /// Pré-carrega capas das linhas prováveis (quota pequena por frame).
    fn prefetch_artwork(&mut self) {
        let mut pending: Vec<(String, Option<String>)> = Vec::new();
        if let Some(row) = self.player.current() {
            pending.push((row.id.clone(), row.artwork_url.clone()));
        }
        for row in self.songs.iter().take(40) {
            pending.push((row.id.clone(), row.artwork_url.clone()));
        }
        for playlist in self.playlists.iter().take(16) {
            pending.push((playlist.id.clone(), playlist.artwork_url.clone()));
        }
        let mut quota = 4_u8;
        for (key, url) in pending {
            if quota == 0 {
                break;
            }
            if self.request_artwork(&key, url.as_deref()) {
                quota -= 1;
            }
        }
    }

    /// Modo efetivo da biblioteca: lista acima do limiar (200 itens).
    fn effective_library_mode(&self) -> ListMode {
        if self.settings.should_use_list_mode(self.songs.len()) {
            ListMode::List
        } else {
            self.library_mode
        }
    }

    /// Modo efetivo das playlists.
    fn effective_playlists_mode(&self) -> ListMode {
        if self.settings.should_use_list_mode(self.playlists.len()) {
            ListMode::List
        } else {
            self.playlists_mode
        }
    }

    /// Toca uma linha de faixa; avisa quando a biblioteca não expõe URL.
    fn play_rows(&mut self, rows: &[SongRow], index: usize) {
        if rows.get(index).is_some_and(|row| row.preview_url.is_none()) {
            self.set_toast(
                BannerKind::Info,
                "Faixas da biblioteca não expõem URL de reprodução; a fila \
                 foi montada e os controles ficam na barra."
                    .to_owned(),
            );
        }
        self.player.play_collection(rows, index);
    }

    /// Atalhos de teclado: Espaço, setas, Ctrl+F e Ctrl+Q.
    fn handle_shortcuts(&mut self, ctx: &Context) {
        let (space, arrow_left, arrow_right, arrow_up, arrow_down, quit, focus_search) =
            ctx.input(|input| {
                (
                    input.key_pressed(egui::Key::Space),
                    input.key_pressed(egui::Key::ArrowLeft),
                    input.key_pressed(egui::Key::ArrowRight),
                    input.key_pressed(egui::Key::ArrowUp),
                    input.key_pressed(egui::Key::ArrowDown),
                    input.modifiers.ctrl && input.key_pressed(egui::Key::Q),
                    input.modifiers.ctrl && input.key_pressed(egui::Key::F),
                )
            });
        if quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        if focus_search && self.has_session() {
            self.screen = Screen::Search;
            ctx.memory_mut(|memory| memory.request_focus(search_edit_id()));
        }
        // Teclas simples só valem fora da digitação de texto.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if space {
            self.player.toggle_play();
        }
        if arrow_left {
            self.player.seek_relative(-10);
        }
        if arrow_right {
            self.player.seek_relative(10);
        }
        if arrow_up {
            self.player.set_volume(self.player.volume() + 0.05);
        }
        if arrow_down {
            self.player.set_volume(self.player.volume() - 0.05);
        }
    }

    /// Rail de navegação (72 px, Início/Biblioteca/Playlists/Busca).
    fn render_rail(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let items = [
            comps::navigation_rail::RailItem::new(Icon::MusicNote, "Início"),
            comps::navigation_rail::RailItem::new(Icon::Grid, "Biblioteca"),
            comps::navigation_rail::RailItem::new(Icon::List, "Playlists"),
            comps::navigation_rail::RailItem::new(Icon::Search, "Busca"),
        ];
        let selected = self.screen.rail_index();
        let responses = egui::Panel::left("sitrus_nav_rail")
            .resizable(false)
            .exact_size(comps::navigation_rail::RAIL_WIDTH)
            .frame(egui::Frame::NONE)
            .show(ui, |rail| {
                comps::navigation_rail::navigation_rail(
                    rail,
                    theme,
                    Id::unique("sitrus_rail"),
                    &items,
                    selected,
                )
            })
            .inner;
        for (index, response) in responses.into_iter().enumerate() {
            if response.clicked() {
                self.screen = match index {
                    0 => Screen::Home,
                    1 => Screen::Library,
                    2 => Screen::Playlists,
                    _ => Screen::Search,
                };
            }
        }
    }

    /// Barra de player fixa no rodapé.
    fn render_player_bar(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let (title, subtitle, _key) = self.player.display();
        let title = if title.is_empty() { "–" } else { title }.to_owned();
        let subtitle = subtitle.to_owned();
        let artwork = self
            .player
            .current()
            .and_then(|row| self.artworks.get(&row.id))
            .cloned();
        let bar_state = comps::player_bar::PlayerBarState {
            title: &title,
            subtitle: &subtitle,
            artwork: artwork.as_ref(),
            playing: self.player.playing(),
            position_secs: self.player.position_secs(),
            duration_secs: self.player.duration_secs(),
            volume: self.player.volume(),
            queue_open: self.queue_open,
            enabled: self.player.has_queue(),
        };
        let bar = egui::Panel::bottom("sitrus_player_bar")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE)
            .show(ui, |row| {
                comps::player_bar::player_bar(
                    row,
                    theme,
                    Id::unique("sitrus_player_bar"),
                    &bar_state,
                )
            })
            .inner;
        if bar.play_pause.clicked() {
            self.player.toggle_play();
        }
        if bar.prev.clicked() {
            self.player.previous();
        }
        if bar.next.clicked() {
            self.player.advance();
        }
        if let Some(fraction) = bar.seek {
            self.player.seek_fraction(fraction);
        }
        if let Some(volume) = bar.volume {
            self.player.set_volume(volume);
        }
        if bar.queue_toggle.clicked() {
            self.queue_open = !self.queue_open;
        }
    }

    /// Overlay do painel de fila, ancorado acima da barra (direita).
    fn render_queue_overlay(&mut self, ctx: &Context, theme: &Theme) {
        if !self.queue_open || self.player.queue_items().is_empty() {
            return;
        }
        let items_text: Vec<(String, String, u32)> = self
            .player
            .queue_items()
            .iter()
            .map(|row| (row.title.clone(), row.subtitle.clone(), row.duration_secs))
            .collect();
        let current_index = self.player.current_index();
        egui::Area::new(Id::unique("sitrus_queue_overlay"))
            .order(egui::Order::Foreground)
            .movable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0_f32, -100.0_f32])
            .default_size(Vec2::new(380.0, 360.0))
            .show(ctx, |panel| {
                let items: Vec<comps::player_bar::QueueItem> = items_text
                    .iter()
                    .enumerate()
                    .map(
                        |(index, (title, subtitle, duration))| comps::player_bar::QueueItem {
                            title,
                            subtitle,
                            duration_secs: *duration,
                            active: current_index == Some(index),
                        },
                    )
                    .collect();
                comps::player_bar::queue_panel(
                    panel,
                    theme,
                    Id::unique("sitrus_queue_panel"),
                    &items,
                );
            });
    }

    /// Conteúdo central por tela + toast.
    fn render_content(&mut self, ui: &mut egui::Ui, theme: &Theme, ctx: &Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg_content))
            .show(ui, |content| {
                match self.screen {
                    Screen::Login => self.render_login(content, theme),
                    Screen::Home => self.render_home(content, theme),
                    Screen::Library => self.render_library(content, theme),
                    Screen::Playlists => self.render_playlists(content, theme),
                    Screen::Search => self.render_search(content, theme, ctx),
                }
                self.render_toast(content, theme);
            });
    }

    /// Toast compartilhado no rodapé da área central.
    fn render_toast(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let Some((kind, message)) = self.toast.as_ref() else {
            return;
        };
        if self.toast_fresh {
            comps::toast::trigger_toast(ui, toast_id(), 3.5);
            self.toast_fresh = false;
        }
        let closed = comps::toast::toast(ui, theme, toast_id(), *kind, message);
        if closed.is_none() || closed.is_some_and(|response| response.clicked()) {
            self.toast = None;
        }
    }

    /// Painel pedindo login (usado nas telas quando sem sessão).
    fn render_login_required(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        ScrollArea::vertical()
            .id_salt("sitrus_login_required")
            .auto_shrink([false, true])
            .show(ui, |scroll| {
                scroll.add_space(spacing::X24);
                let action = comps::empty_state::empty_state(
                    scroll,
                    theme,
                    Id::unique("sitrus_empty_login_required"),
                    Icon::MusicNote,
                    "Entre com sua conta da Apple Music",
                    "O Sitrus lê a sua biblioteca com a sessão do player web \
                     da Apple. Sem sessão, as telas ficam vazias.",
                    Some("Entrar"),
                );
                if let Some(button) = action
                    && button.clicked()
                {
                    self.screen = Screen::Login;
                }
            });
    }

    /// Tela Início: hero + prateleiras.
    fn render_home(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        if !self.has_session() {
            self.render_login_required(ui, theme);
            return;
        }
        ScrollArea::vertical()
            .id_salt("sitrus_screen_home")
            .auto_shrink([false, true])
            .show(ui, |scroll| {
                scroll.add_space(spacing::X8);
                let songs_state = self.songs_state.clone();
                match songs_state {
                    SectionState::Loading => {
                        let hero = comps::hero_header::HeroData::new("Sitrus", None);
                        comps::hero_header::hero_header(
                            scroll,
                            theme,
                            Id::unique("sitrus_home_hero_loading"),
                            None,
                            &hero,
                            false,
                        );
                        scroll.add_space(spacing::X16);
                        comps::skeleton_row::skeleton_rows(
                            scroll,
                            theme,
                            Id::unique("sitrus_home_skeleton"),
                            SKELETON_ROWS,
                        );
                    }
                    SectionState::Failed(message) => {
                        scroll.add_space(spacing::X16);
                        let close = comps::error_banner::error_banner(
                            scroll,
                            theme,
                            Id::unique("sitrus_home_error_banner"),
                            BannerKind::Error,
                            &message,
                            true,
                        );
                        if let Some(button) = close
                            && button.clicked()
                        {
                            self.songs_state = SectionState::Loaded;
                        }
                        scroll.add_space(spacing::X16);
                        let retry = comps::empty_state::empty_state(
                            scroll,
                            theme,
                            Id::unique("sitrus_home_error_empty"),
                            Icon::Warning,
                            "Não foi possível carregar sua biblioteca",
                            "Verifique a conexão e tente novamente.",
                            Some("Tentar de novo"),
                        );
                        if let Some(button) = retry
                            && button.clicked()
                        {
                            self.refresh_library();
                        }
                    }
                    SectionState::Loaded if self.songs.is_empty() => {
                        scroll.add_space(spacing::X24);
                        comps::empty_state::empty_state(
                            scroll,
                            theme,
                            Id::unique("sitrus_home_empty"),
                            Icon::MusicNote,
                            "Sua biblioteca está vazia",
                            "Adicione músicas na Apple Music e elas aparecem \
                             aqui.",
                            None,
                        );
                    }
                    SectionState::Loaded => {
                        self.render_home_loaded(scroll, theme);
                    }
                }
            });
    }

    /// Home com dados: hero do item em destaque + três prateleiras.
    fn render_home_loaded(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let recent: Vec<SongRow> = self.songs.iter().take(HOME_SHELF_ITEMS).cloned().collect();
        if let Some(featured) = recent.first() {
            let artwork = self.artworks.get(&featured.id).cloned();
            let gradient = self.gradients.get(&featured.id).copied();
            let featured_id = featured.id.clone();
            let featured_title = featured.title.clone();
            let featured_subtitle = featured.subtitle.clone();
            let mut hero = comps::hero_header::HeroData::new(&featured_title, gradient);
            hero.subtitle = Some(&featured_subtitle);
            let play = comps::hero_header::hero_header(
                ui,
                theme,
                Id::unique("sitrus_home_hero").with(&featured_id),
                artwork.as_ref(),
                &hero,
                true,
            );
            if play.clicked() {
                self.play_rows(&recent, 0);
                self.queue_open = false;
            }
            ui.add_space(spacing::X16);
        } else {
            return;
        }

        // Adicionados recentemente / Mais tocadas (fallback sem playCount no
        // modelo de sitrus-api) / Suas playlists.
        let reversed: Vec<SongRow> = recent.iter().rev().cloned().collect();
        self.render_song_shelf(
            ui,
            theme,
            Id::unique("sitrus_shelf_recent"),
            "Adicionados recentemente",
            &recent,
        );
        ui.add_space(spacing::X8);
        self.render_song_shelf(
            ui,
            theme,
            Id::unique("sitrus_shelf_top"),
            "Mais tocadas",
            &reversed,
        );
        ui.add_space(spacing::X8);
        self.render_playlist_shelf(
            ui,
            theme,
            Id::unique("sitrus_shelf_playlists"),
            "Suas playlists",
        );
        ui.add_space(spacing::X24);
    }

    /// Prateleira de faixas (cards) com clique para tocar.
    fn render_song_shelf(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        shelf_id: Id,
        title: &str,
        rows: &[SongRow],
    ) {
        let clicked: Rc<RefCell<Vec<usize>>> = Rc::default();
        let artworks = &self.artworks;
        let output =
            comps::shelf_row::shelf_row(ui, theme, shelf_id, title, rows.len(), |cell, index| {
                let row = &rows[index];
                let artwork = artworks.get(&row.id).cloned();
                let response = comps::album_card::album_card(
                    cell,
                    theme,
                    Id::unique("sitrus_shelf_card").with(("card", index)),
                    comps::album_card::AlbumCard {
                        artwork: artwork.as_ref(),
                        title: &row.title,
                        subtitle: &row.subtitle,
                        enabled: true,
                        selected: false,
                        width: comps::album_card::CARD_WIDTH,
                    },
                );
                if response.clicked() {
                    clicked.borrow_mut().push(index);
                }
                response
            });
        let _ = output.viewport_width;
        if output.see_all.clicked() {
            self.screen = Screen::Library;
        }
        let hits = clicked.borrow().clone();
        if let Some(&index) = hits.first() {
            self.play_rows(rows, index);
        }
    }

    /// Prateleira de playlists (cards ou mensagem de vazio).
    fn render_playlist_shelf(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        shelf_id: Id,
        title: &str,
    ) {
        let playlists = &self.playlists;
        match &self.playlists_state {
            SectionState::Loading => {
                comps::skeleton_row::skeleton_rows(
                    ui,
                    theme,
                    Id::unique("sitrus_shelf_playlists_skeleton"),
                    2,
                );
            }
            SectionState::Failed(message) => {
                comps::error_banner::error_banner(
                    ui,
                    theme,
                    Id::unique("sitrus_shelf_playlists_error"),
                    BannerKind::Error,
                    message,
                    false,
                );
            }
            SectionState::Loaded if playlists.is_empty() => {
                ui.label(
                    egui::RichText::new("Nenhuma playlist na sua biblioteca ainda.")
                        .font(typography::font_id(type_scale::TypeToken::Body))
                        .color(theme.label_secondary),
                );
            }
            SectionState::Loaded => {
                let clicked: Rc<RefCell<Vec<usize>>> = Rc::default();
                let artworks = &self.artworks;
                let output = comps::shelf_row::shelf_row(
                    ui,
                    theme,
                    shelf_id,
                    title,
                    playlists.len(),
                    |cell, index| {
                        let playlist = &playlists[index];
                        let artwork = artworks.get(&playlist.id).cloned();
                        let subtitle = playlist_subtitle(playlist);
                        let response = comps::album_card::album_card(
                            cell,
                            theme,
                            Id::unique("sitrus_playlist_card").with(("card", index)),
                            comps::album_card::AlbumCard {
                                artwork: artwork.as_ref(),
                                title: &playlist.name,
                                subtitle: &subtitle,
                                enabled: true,
                                selected: false,
                                width: comps::album_card::CARD_WIDTH,
                            },
                        );
                        if response.clicked() {
                            clicked.borrow_mut().push(index);
                        }
                        response
                    },
                );
                let _ = output.viewport_width;
                if output.see_all.clicked() {
                    self.screen = Screen::Playlists;
                }
                let hits = clicked.borrow().clone();
                if !hits.is_empty() {
                    self.set_toast(
                        BannerKind::Info,
                        "Abrir o conteúdo da playlist depende de um endpoint \
                         ainda não exposto pela camada de API (ver release \
                         notes)."
                            .to_owned(),
                    );
                }
            }
        }
    }

    /// Tela Biblioteca: alternância prateleiras/lista + auto modo lista.
    fn render_library(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        if !self.has_session() {
            self.render_login_required(ui, theme);
            return;
        }
        ui.add_space(spacing::X16);
        padded(ui, |header| {
            let mode = self.effective_library_mode();
            let chosen = comps::segmented_control::segmented_control(
                header,
                theme,
                Id::unique("sitrus_library_mode"),
                &ListMode::SEGMENT_LABELS,
                mode.segment_index(),
                true,
            );
            self.library_mode = if chosen == 0 {
                ListMode::Shelves
            } else {
                ListMode::List
            };
            if !self.songs_complete && !self.songs.is_empty() {
                header.add_space(spacing::X12);
                header.label(
                    egui::RichText::new(format!(
                        "Mostrando as primeiras {} faixas",
                        self.songs.len()
                    ))
                    .font(typography::font_id(type_scale::TypeToken::Footnote))
                    .color(theme.label_tertiary),
                );
            }
        });
        ui.add_space(spacing::X4);
        let songs_state = self.songs_state.clone();
        match songs_state {
            SectionState::Loading => {
                padded(ui, |body| {
                    comps::skeleton_row::skeleton_rows(
                        body,
                        theme,
                        Id::unique("sitrus_library_skeleton"),
                        SKELETON_ROWS,
                    );
                });
            }
            SectionState::Failed(message) => {
                padded(ui, |body| {
                    let message = message.clone();
                    let retry =
                        render_section_error(body, theme, "library", &message, "Tentar de novo");
                    if let Some(button) = retry
                        && button.clicked()
                    {
                        self.refresh_library();
                    }
                });
            }
            SectionState::Loaded if self.songs.is_empty() => {
                padded(ui, |body| {
                    body.add_space(spacing::X16);
                    comps::empty_state::empty_state(
                        body,
                        theme,
                        Id::unique("sitrus_library_empty"),
                        Icon::MusicNote,
                        "Biblioteca vazia",
                        "Nenhuma faixa encontrada na sua biblioteca da Apple \
                         Music.",
                        None,
                    );
                });
            }
            SectionState::Loaded => {
                ScrollArea::vertical()
                    .id_salt("sitrus_library_content")
                    .auto_shrink([false, true])
                    .show(ui, |scroll| {
                        scroll.add_space(spacing::X8);
                        let mode = if self.settings.should_use_list_mode(self.songs.len()) {
                            ListMode::List
                        } else {
                            ListMode::Shelves
                        };
                        let clicked = if mode == ListMode::Shelves {
                            render_songs_grid(scroll, theme, &self.songs, &self.artworks)
                        } else {
                            render_songs_list(scroll, theme, &self.songs, &self.artworks)
                        };
                        if let Some(index) = clicked {
                            let rows = self.songs.clone();
                            self.play_rows(&rows, index);
                        }
                        scroll.add_space(spacing::X24);
                    });
            }
        }
    }

    /// Tela Playlists: alternância prateleiras/lista + auto modo lista.
    fn render_playlists(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        if !self.has_session() {
            self.render_login_required(ui, theme);
            return;
        }
        ui.add_space(spacing::X16);
        padded(ui, |header| {
            let mode = self.effective_playlists_mode();
            let chosen = comps::segmented_control::segmented_control(
                header,
                theme,
                Id::unique("sitrus_playlists_mode"),
                &ListMode::SEGMENT_LABELS,
                mode.segment_index(),
                true,
            );
            self.playlists_mode = if chosen == 0 {
                ListMode::Shelves
            } else {
                ListMode::List
            };
        });
        ui.add_space(spacing::X4);
        let playlists_state = self.playlists_state.clone();
        match playlists_state {
            SectionState::Loading => {
                padded(ui, |body| {
                    comps::skeleton_row::skeleton_rows(
                        body,
                        theme,
                        Id::unique("sitrus_playlists_skeleton"),
                        SKELETON_ROWS,
                    );
                });
            }
            SectionState::Failed(message) => {
                padded(ui, |body| {
                    let message = message.clone();
                    let retry =
                        render_section_error(body, theme, "playlists", &message, "Tentar de novo");
                    if let Some(button) = retry
                        && button.clicked()
                    {
                        self.refresh_library();
                    }
                });
            }
            SectionState::Loaded if self.playlists.is_empty() => {
                padded(ui, |body| {
                    body.add_space(spacing::X16);
                    comps::empty_state::empty_state(
                        body,
                        theme,
                        Id::unique("sitrus_playlists_empty"),
                        Icon::List,
                        "Nenhuma playlist",
                        "Crie playlists na Apple Music e elas aparecem aqui.",
                        None,
                    );
                });
            }
            SectionState::Loaded => {
                ScrollArea::vertical()
                    .id_salt("sitrus_playlists_content")
                    .auto_shrink([false, true])
                    .show(ui, |scroll| {
                        scroll.add_space(spacing::X8);
                        let mode = if self.settings.should_use_list_mode(self.playlists.len()) {
                            ListMode::List
                        } else {
                            ListMode::Shelves
                        };
                        let opened = if mode == ListMode::Shelves {
                            render_playlists_grid(scroll, theme, &self.playlists, &self.artworks)
                        } else {
                            render_playlists_list(scroll, theme, &self.playlists)
                        };
                        if opened.is_some() {
                            self.set_toast(
                                BannerKind::Info,
                                "Abrir o conteúdo da playlist depende de um \
                                 endpoint ainda não exposto pela camada de \
                                 API (ver release notes)."
                                    .to_owned(),
                            );
                        }
                        scroll.add_space(spacing::X24);
                    });
            }
        }
    }

    /// Tela Busca: SearchField fixo + resultados roláveis.
    fn render_search(&mut self, ui: &mut egui::Ui, theme: &Theme, ctx: &Context) {
        if !self.has_session() {
            self.render_login_required(ui, theme);
            return;
        }
        ui.add_space(spacing::X16);
        padded(ui, |field_row| {
            comps::search_field::search_field(
                field_row,
                theme,
                search_field_id(),
                &mut self.search.input,
                true,
            );
        });
        // Enter dentro do campo pesquisa; Ctrl+F foca o campo (atalhos).
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let editing = ctx.memory(|memory| memory.has_focus(search_edit_id()));
        if enter && editing {
            self.submit_search();
        }
        if !self.search.results.is_empty() && !self.search.source.is_empty() {
            padded(ui, |hint_row| {
                hint_row.add_space(spacing::X4);
                let mut hint = format!("Origem: {}", self.search.source);
                if let Some(note) = &self.search.note {
                    hint.push_str(&format!(" — {note}"));
                }
                hint_row.label(
                    egui::RichText::new(hint)
                        .font(typography::font_id(type_scale::TypeToken::Footnote))
                        .color(theme.label_tertiary),
                );
            });
        }
        ui.add_space(spacing::X8);
        let search_state = self.search.state.clone();
        match search_state {
            SectionState::Loading => {
                padded(ui, |body| {
                    comps::skeleton_row::skeleton_rows(
                        body,
                        theme,
                        Id::unique("sitrus_search_skeleton"),
                        6,
                    );
                });
            }
            SectionState::Failed(message) => {
                padded(ui, |body| {
                    let message = message.clone();
                    let retry =
                        render_section_error(body, theme, "search", &message, "Buscar de novo");
                    if let Some(button) = retry
                        && button.clicked()
                    {
                        self.submit_search();
                    }
                });
            }
            SectionState::Loaded
                if self.search.input.is_empty() && self.search.results.is_empty() =>
            {
                padded(ui, |body| {
                    body.add_space(spacing::X16);
                    comps::empty_state::empty_state(
                        body,
                        theme,
                        Id::unique("sitrus_search_idle"),
                        Icon::Search,
                        "Buscar músicas",
                        "Digite para buscar no catálogo da Apple Music; sem \
                         rede, o filtro roda na sua biblioteca.",
                        None,
                    );
                });
            }
            SectionState::Loaded if self.search.results.is_empty() => {
                padded(ui, |body| {
                    body.add_space(spacing::X16);
                    comps::empty_state::empty_state(
                        body,
                        theme,
                        Id::unique("sitrus_search_empty"),
                        Icon::Search,
                        "Nada encontrado",
                        &format!("Nenhum item corresponde a “{}”.", self.search.submitted),
                        None,
                    );
                });
            }
            SectionState::Loaded => {
                ScrollArea::vertical()
                    .id_salt("sitrus_search_results")
                    .auto_shrink([false, true])
                    .show(ui, |scroll| {
                        scroll.add_space(spacing::X8);
                        let results = &self.search.results;
                        let artworks = &self.artworks;
                        let clicked: Rc<RefCell<Option<usize>>> = Rc::default();
                        for (index, row) in results.iter().enumerate() {
                            let artwork = artworks.get(&row.id).cloned();
                            let response = comps::list_row::song_row(
                                scroll,
                                theme,
                                Id::unique("sitrus_search_row").with(("row", index)),
                                comps::list_row::ListRow {
                                    index: Some(index),
                                    title: &row.title,
                                    subtitle: Some(&row.subtitle),
                                    duration_secs: if row.duration_secs > 0 {
                                        Some(row.duration_secs)
                                    } else {
                                        None
                                    },
                                    artwork: artwork.as_ref(),
                                    hover_icon: None,
                                    hover_tooltip: None,
                                },
                                true,
                                false,
                            );
                            if response.clicked() {
                                *clicked.borrow_mut() = Some(index);
                            }
                        }
                        let hits = *clicked.borrow();
                        if let Some(index) = hits {
                            let rows = self.search.results.clone();
                            self.play_rows(&rows, index);
                        }
                        scroll.add_space(spacing::X24);
                    });
            }
        }
    }

    /// Tela Entrar (login manual sempre; webview atrás da feature).
    fn render_login(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let logged = self.has_session();
        let action = login::render_login_screen(ui, theme, &mut self.login, logged);
        match action {
            LoginAction::None => {}
            LoginAction::Save(text) => self.on_manual_login(&text),
            LoginAction::OpenWebview => self.open_webview_login(),
            LoginAction::Logout => self.logout(),
        }
    }
}

/// Erro de seção: banner + estado vazio com ação; devolve a resposta da ação.
fn render_section_error(
    ui: &mut egui::Ui,
    theme: &Theme,
    section: &'static str,
    message: &str,
    action_label: &str,
) -> Option<egui::Response> {
    ui.add_space(spacing::X12);
    comps::error_banner::error_banner(
        ui,
        theme,
        Id::unique(("sitrus_section_error", section)),
        BannerKind::Error,
        message,
        false,
    );
    ui.add_space(spacing::X16);
    comps::empty_state::empty_state(
        ui,
        theme,
        Id::unique(("sitrus_section_retry", section)),
        Icon::Warning,
        "Algo deu errado",
        "Tente novamente em instantes.",
        Some(action_label),
    )
}

/// Subtítulo de playlist ("N faixas" quando a resposta traz contagem).
fn playlist_subtitle(playlist: &PlaylistRow) -> String {
    match playlist.track_count {
        Some(count) => format!("{count} faixas"),
        None => "Playlist".to_owned(),
    }
}

impl eframe::App for App {
    /// Frame completa: tema, eventos, atalhos, rail, conteúdo, player e fila.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        typography::install_once(&ctx);
        let theme = self.current_theme(&ctx);
        theme.apply(&ctx);

        self.drain_events(&ctx);
        self.handle_shortcuts(&ctx);

        // Ordem dos painéis importa: o rodapé (player bar 88 px) é o
        // panel mais externo (largura total) e o rail de 72 px fica acima dele.
        self.render_player_bar(ui, &theme);
        self.render_rail(ui, &theme);
        self.render_content(ui, &theme, &ctx);
        self.render_queue_overlay(&ctx, &theme);

        self.prefetch_artwork();

        // Repaint contínuo quando algo pode mudar sem input do usuário:
        // skeletons/shimmer, capas pendentes, reprodução, toast, webview.
        let animating = !self.pending_artwork.is_empty()
            || self.songs_state.is_loading()
            || self.playlists_state.is_loading()
            || self.search.state.is_loading()
            || self.player.playing()
            || self.toast.is_some()
            || self.login.webview_open;
        if animating {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

/// Grade de cards de faixas (modo prateleiras da biblioteca); devolve o
/// índice clicado, se houver.
fn render_songs_grid(
    scroll: &mut egui::Ui,
    theme: &Theme,
    songs: &[SongRow],
    artworks: &HashMap<String, TextureHandle>,
) -> Option<usize> {
    let inner = scroll.available_width().max(280.0) - 2.0 * CONTENT_X24;
    let card_step = comps::album_card::CARD_WIDTH + spacing::X16;
    let columns = ((inner / card_step).floor() as usize).max(1);
    let clicked: Rc<RefCell<Option<usize>>> = Rc::default();
    egui::Grid::new(Id::unique("sitrus_library_grid"))
        .spacing([spacing::X16, spacing::X16])
        .show(scroll, |grid| {
            for (index, row) in songs.iter().enumerate() {
                let artwork = artworks.get(&row.id).cloned();
                let response = comps::album_card::album_card(
                    grid,
                    theme,
                    Id::unique("sitrus_library_card").with(("card", index)),
                    comps::album_card::AlbumCard {
                        artwork: artwork.as_ref(),
                        title: &row.title,
                        subtitle: &row.subtitle,
                        enabled: true,
                        selected: false,
                        width: comps::album_card::CARD_WIDTH,
                    },
                );
                if response.clicked() {
                    *clicked.borrow_mut() = Some(index);
                }
                if (index + 1) % columns == 0 {
                    grid.end_row();
                }
            }
        });
    *clicked.borrow()
}

/// Lista densa de faixas (modo lista da biblioteca); devolve o índice
/// clicado, se houver.
fn render_songs_list(
    scroll: &mut egui::Ui,
    theme: &Theme,
    songs: &[SongRow],
    artworks: &HashMap<String, TextureHandle>,
) -> Option<usize> {
    let clicked: Rc<RefCell<Option<usize>>> = Rc::default();
    for (index, row) in songs.iter().enumerate() {
        let artwork = artworks.get(&row.id).cloned();
        let response = comps::list_row::song_row(
            scroll,
            theme,
            Id::unique("sitrus_library_row").with(("row", index)),
            comps::list_row::ListRow {
                index: Some(index),
                title: &row.title,
                subtitle: Some(&row.subtitle),
                duration_secs: if row.duration_secs > 0 {
                    Some(row.duration_secs)
                } else {
                    None
                },
                artwork: artwork.as_ref(),
                hover_icon: None,
                hover_tooltip: None,
            },
            true,
            false,
        );
        if response.clicked() {
            *clicked.borrow_mut() = Some(index);
        }
    }
    *clicked.borrow()
}

/// Grade de cards de playlists (modo prateleiras); devolve o índice aberto.
fn render_playlists_grid(
    scroll: &mut egui::Ui,
    theme: &Theme,
    playlists: &[PlaylistRow],
    artworks: &HashMap<String, TextureHandle>,
) -> Option<usize> {
    let inner = scroll.available_width().max(280.0) - 2.0 * CONTENT_X24;
    let card_step = comps::album_card::CARD_WIDTH + spacing::X16;
    let columns = ((inner / card_step).floor() as usize).max(1);
    let opened: Rc<RefCell<Option<usize>>> = Rc::default();
    egui::Grid::new(Id::unique("sitrus_playlists_grid"))
        .spacing([spacing::X16, spacing::X16])
        .show(scroll, |grid| {
            for (index, playlist) in playlists.iter().enumerate() {
                let artwork = artworks.get(&playlist.id).cloned();
                let subtitle = playlist_subtitle(playlist);
                let response = comps::album_card::album_card(
                    grid,
                    theme,
                    Id::unique("sitrus_playlists_card").with(("card", index)),
                    comps::album_card::AlbumCard {
                        artwork: artwork.as_ref(),
                        title: &playlist.name,
                        subtitle: &subtitle,
                        enabled: true,
                        selected: false,
                        width: comps::album_card::CARD_WIDTH,
                    },
                );
                if response.clicked() {
                    *opened.borrow_mut() = Some(index);
                }
                if (index + 1) % columns == 0 {
                    grid.end_row();
                }
            }
        });
    *opened.borrow()
}

/// Lista de playlists (modo lista); devolve o índice aberto.
fn render_playlists_list(
    scroll: &mut egui::Ui,
    theme: &Theme,
    playlists: &[PlaylistRow],
) -> Option<usize> {
    let opened: Rc<RefCell<Option<usize>>> = Rc::default();
    for (index, playlist) in playlists.iter().enumerate() {
        let response = comps::list_row::list_row(
            scroll,
            theme,
            Id::unique("sitrus_playlists_row").with(("row", index)),
            comps::list_row::ListRow {
                index: Some(index),
                title: &playlist.name,
                subtitle: Some(&playlist_subtitle(playlist)),
                duration_secs: None,
                artwork: None,
                hover_icon: None,
                hover_tooltip: None,
            },
            true,
            false,
        );
        if response.clicked() {
            *opened.borrow_mut() = Some(index);
        }
    }
    *opened.borrow()
}
