//! Galeria de desenvolvimento: todos os componentes nos dois temas.
//!
//! Tela obrigatória de desenvolvimento (design system): permite revisar a UI
//! sem dependência de login — `sitrus-ui::gallery(ui)` cobre cada componente
//! com seus estados, em tema escuro e claro.

use egui::{Align, Id, Layout, Ui};

use crate::anim;
use crate::artwork;
use crate::components::{
    album_card, button, detail_header, empty_state, error_banner, hero_header, list_row, modal,
    navigation_rail, player_bar, search_field, section_header, segmented_control, shelf_row,
    skeleton_row, tabs, toast, track_table,
};
use crate::icons::Icon;
use crate::theme::{Theme, corner_radius};
use crate::tokens::{self, spacing, type_scale};
use crate::typography;

fn theme_key() -> Id {
    Id::unique("sitrus_gallery_dark")
}
fn modal_key() -> Id {
    Id::unique("sitrus_gallery_modal")
}
fn search_key() -> Id {
    Id::unique("sitrus_gallery_search")
}
fn playing_key() -> Id {
    Id::unique("sitrus_gallery_playing")
}
fn queue_key() -> Id {
    Id::unique("sitrus_gallery_queue")
}
fn toast_key() -> Id {
    Id::unique("gallery.toast")
}

/// Renderiza a galeria completa dentro do `ui` fornecido.
pub fn gallery(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    typography::install_once(&ctx);

    let mut dark: bool = ctx
        .data_mut(|d| d.get_temp::<bool>(theme_key()))
        .unwrap_or(true);

    // Cabeçalho + alternador de tema.
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Galeria do design system")
                .font(typography::font_id(type_scale::TypeToken::LargeTitle))
                .color(Theme::dark().label_primary),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if dark {
                "Ver tema claro"
            } else {
                "Ver tema escuro"
            };
            if button_toggle(ui, dark, label).clicked() {
                dark = !dark;
                ctx.data_mut(|d| d.insert_temp(theme_key(), dark));
            }
        });
    });
    ui.add_space(spacing::X8);

    // Espécime tipográfico no tema corrente.
    let current = if dark { Theme::dark() } else { Theme::light() };
    ui.scope(|ui| {
        current.apply_ui(ui);
        ui.scope(|ui| {
            ui.label(
                egui::RichText::new("Tipografia (Inter variável)")
                    .font(typography::font_id(type_scale::TypeToken::Title2))
                    .color(current.label_secondary),
            );
            for token in type_scale::TypeToken::all() {
                let spec = token.spec();
                ui.label(
                    egui::RichText::new(format!(
                        "{token:?} — {} px / {} lh — peso {}",
                        spec.size,
                        spec.line_height,
                        spec.weight.value()
                    ))
                    .font(typography::font_id(*token))
                    .color(current.label_primary),
                );
            }
        });
    });
    ui.add_space(spacing::X16);

    // Cartões de preview dos dois temas.
    preview_card(ui, &Theme::dark(), Id::unique("gallery.dark"));
    ui.add_space(spacing::X16);
    preview_card(ui, &Theme::light(), Id::unique("gallery.light"));
    ui.add_space(spacing::X16);

    // Toast global (disparado por botões nos cartões acima).
    ui.scope(|ui| {
        current.apply_ui(ui);
        if toast::toast(
            ui,
            &current,
            toast_key(),
            error_banner::BannerKind::Info,
            "Adicionado à fila de reprodução",
        )
        .is_none()
        {
            ui.label(
                egui::RichText::new("(toast expirado — use “Toast de sucesso” para reanimar)")
                    .font(typography::font_id(type_scale::TypeToken::Footnote))
                    .color(current.label_tertiary),
            );
        }
    });

    // Modal de demonstração (aberto por botão nos cartões).
    let modal_open: bool = ctx
        .data_mut(|d| d.get_temp::<bool>(modal_key()))
        .unwrap_or(false);
    if modal_open {
        let outcome = modal::modal(
            ui,
            &current,
            Id::unique("gallery.modal"),
            "Confirmar ação",
            |ui| {
                ui.label(
                    egui::RichText::new("Exemplo de confirmação (Sheet) do design system.")
                        .font(typography::font_id(type_scale::TypeToken::Body))
                        .color(current.label_primary),
                );
            },
            "Confirmar",
        );
        if outcome.dismissed || outcome.confirmed {
            ctx.data_mut(|d| d.insert_temp::<bool>(modal_key(), false));
        }
    }
}

fn button_toggle(ui: &mut Ui, _dark: bool, label: &str) -> egui::Response {
    let theme = Theme::dark();
    button::button(
        ui,
        &theme,
        Id::unique("gallery.theme_toggle"),
        button::ButtonVariant::Secondary,
        button::ButtonContent::label(label),
        true,
        false,
    )
}

/// Cartão completo de preview para um tema.
fn preview_card(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.scope(|ui| {
        theme.apply_ui(ui);
        egui::Frame::default()
            .fill(theme.bg_window)
            .corner_radius(corner_radius(tokens::radius::LG))
            .inner_margin(spacing::X16)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(if theme.dark_mode {
                        "Tema escuro"
                    } else {
                        "Tema claro"
                    })
                    .font(typography::font_id(type_scale::TypeToken::Title2))
                    .color(theme.label_secondary),
                );
                ui.add_space(spacing::X12);

                rail_section(ui, theme, base);
                ui.add_space(spacing::X16);

                hero_section(ui, theme, base);
                ui.add_space(spacing::X16);

                shelf_section(ui, theme, base);
                ui.add_space(spacing::X16);

                buttons_section(ui, theme, base);
                ui.add_space(spacing::X16);

                controls_section(ui, theme, base);
                ui.add_space(spacing::X16);

                rows_section(ui, theme, base);
                ui.add_space(spacing::X16);

                detail_pages_section(ui, theme, base);
                ui.add_space(spacing::X16);

                player_section(ui, theme, base);
                ui.add_space(spacing::X16);

                loading_feedback_section(ui, theme, base);
            });
    });
}

fn rail_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("NavigationRail (72 px, ícone + tooltip)")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    ui.horizontal_top(|ui| {
        let items: Vec<navigation_rail::RailItem<'_>> = vec![
            navigation_rail::RailItem::new(Icon::List, "Início"),
            navigation_rail::RailItem::new(Icon::Grid, "Biblioteca"),
            navigation_rail::RailItem::new(Icon::Queue, "Playlists"),
            navigation_rail::RailItem::new(Icon::Search, "Busca"),
            navigation_rail::RailItem {
                icon: Icon::MusicNote,
                label: "Logado? (breve)",
                enabled: false,
            },
        ];
        navigation_rail::navigation_rail(ui, theme, base.with("rail"), &items, Some(0));
    });
}

fn hero_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("HeroHeader (artwork 320, gradiente, botão de tocar)")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    let cover = demo_artwork(ui.ctx(), base.with("hero_art"), 11);
    let gradient = hero_gradient_of(11);
    let hero = hero_header::HeroData {
        title: "Midnight Citrus",
        subtitle: Some("Álbum · Sitrus Studio Players"),
        action_label: None,
        gradient: Some(gradient),
    };
    let play = hero_header::hero_header(ui, theme, base.with("hero"), Some(&cover), &hero, true);
    if play.clicked() {
        toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
    }
    ui.add_space(spacing::X8);
    // Hero sem artwork: tokens de gradiente do tema.
    let hero2 = hero_header::HeroData::new("Playlist — Foco Profundo", None);
    let _ = hero_header::hero_header(ui, theme, base.with("hero2"), None, &hero2, false);
}

fn shelf_section(ui: &mut Ui, theme: &Theme, base: Id) {
    section_header::section_header(
        ui,
        theme,
        base.with("shelf_header"),
        "Feito para você",
        Some("Ver tudo"),
    );
    let mut textures: Vec<Option<egui::TextureHandle>> = Vec::new();
    for seed in 0..8 {
        textures.push(Some(demo_artwork(
            ui.ctx(),
            base.with(("card_art", seed)),
            seed + 20,
        )));
    }
    shelf_row::shelf_row(ui, theme, base.with("shelf"), "Prateleiras", 8, |ui, i| {
        let title = format!("Álbum {i:02}");
        album_card::album_card(
            ui,
            theme,
            base.with(("card", i)),
            album_card::AlbumCard {
                artwork: textures[i].as_ref(),
                title: &title,
                subtitle: "Artista demo",
                enabled: true,
                selected: i == 2,
                width: album_card::CARD_WIDTH,
            },
        );
    });
    ui.add_space(spacing::X4);
    shelf_row::shelf_row(
        ui,
        theme,
        base.with("shelf2"),
        "Sem artwork (placeholders)",
        5,
        |ui, i| {
            album_card::album_card(
                ui,
                theme,
                base.with(("card2", i)),
                album_card::AlbumCard {
                    artwork: None,
                    title: &format!("Album placeholder {i}"),
                    subtitle: "Artista",
                    enabled: i != 4,
                    selected: false,
                    width: album_card::CARD_WIDTH,
                },
            );
        },
    );
}

fn buttons_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("Button — variantes ao vivo")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    ui.horizontal(|ui| {
        button::button(
            ui,
            theme,
            base.with("btn_primary_live"),
            button::ButtonVariant::Primary,
            button::ButtonContent::label("Primário"),
            true,
            false,
        );
        button::button(
            ui,
            theme,
            base.with("btn_secondary_live"),
            button::ButtonVariant::Secondary,
            button::ButtonContent::icon_and_text(Icon::Add, "Secundário"),
            true,
            false,
        );
        button::button(
            ui,
            theme,
            base.with("btn_plain_live"),
            button::ButtonVariant::Plain,
            button::ButtonContent::label("Plain"),
            true,
            false,
        );
        button::button(
            ui,
            theme,
            base.with("btn_icon_live"),
            button::ButtonVariant::Icon,
            button::ButtonContent::glyph(Icon::Play),
            true,
            false,
        );
    });
    ui.add_space(spacing::X8);

    // Inventário estático de estados por variante.
    ui.label(
        egui::RichText::new("Button — todos os estados por variante")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    let states = [
        ("Normal", button::ButtonState::Normal),
        ("Hovered", button::ButtonState::Hovered),
        ("Pressed", button::ButtonState::Pressed),
        ("Focused", button::ButtonState::Focused),
        ("Disabled", button::ButtonState::Disabled),
        ("Selected", button::ButtonState::Selected),
    ];
    let variants = [
        ("Primary", button::ButtonVariant::Primary),
        ("Secondary", button::ButtonVariant::Secondary),
        ("Plain", button::ButtonVariant::Plain),
        ("Icon", button::ButtonVariant::Icon),
    ];
    egui::Grid::new(base.with("button_states_grid"))
        .spacing([spacing::X8, spacing::X8])
        .show(ui, |grid| {
            grid.label(
                egui::RichText::new("Estado →")
                    .font(typography::font_id(type_scale::TypeToken::Caption))
                    .color(theme.label_tertiary),
            );
            for (name, _) in states {
                grid.label(
                    egui::RichText::new(name)
                        .font(typography::font_id(type_scale::TypeToken::Caption))
                        .color(theme.label_tertiary),
                );
            }
            grid.end_row();

            for (variant_name, variant) in variants {
                grid.label(
                    egui::RichText::new(variant_name)
                        .font(typography::font_id(type_scale::TypeToken::Caption))
                        .color(theme.label_secondary),
                );
                for (state_name, state) in states {
                    let (rect, _) = grid
                        .allocate_exact_size(egui::Vec2::new(110.0, 44.0), egui::Sense::hover());
                    let content = match variant {
                        button::ButtonVariant::Icon => button::ButtonContent::glyph(Icon::Play),
                        _ => button::ButtonContent::label(state_name),
                    };
                    button::paint_button(grid, theme, rect, variant, state, content);
                }
                grid.end_row();
            }
        });
}

fn controls_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("SegmentedControl e SearchField")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    let selected: usize = ui
        .ctx()
        .data_mut(|d| d.get_temp(base.with("segmented")))
        .unwrap_or(1);
    let next = segmented_control::segmented_control(
        ui,
        theme,
        base.with("segmented"),
        &["Prateleiras", "Lista"],
        selected,
        true,
    );
    if next != selected {
        ui.ctx()
            .data_mut(|d| d.insert_temp(base.with("segmented"), next));
    }
    ui.add_space(spacing::X8);

    let mut query: String = ui
        .ctx()
        .data_mut(|d| d.get_temp(search_key()).unwrap_or_default());
    search_field::search_field(ui, theme, search_key(), &mut query, true);
    // Campo desabilitado também.
    let mut disabled_query = String::new();
    search_field::search_field(
        ui,
        theme,
        search_key().with("disabled"),
        &mut disabled_query,
        false,
    );
    ui.ctx().data_mut(|d| d.insert_temp(search_key(), query));

    ui.add_space(spacing::X8);
    // Estados vazios.
    empty_state::empty_state(
        ui,
        theme,
        base.with("empty"),
        Icon::MusicNote,
        "Nada por aqui ainda",
        "Quando os resultados chegarem, aparece aqui.",
        Some("Buscar na Apple Music"),
    );
    let _ = Id::unique("unused");
}

fn rows_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("ListRow / SongRow (44 px, ação de hover)")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    let cover = demo_artwork(ui.ctx(), base.with("row_art"), 42);
    let rows: Vec<list_row::ListRow<'_>> = vec![
        list_row::ListRow {
            index: Some(0),
            title: "Cidade do Interior",
            subtitle: Some("Banda Auroral"),
            duration_secs: Some(187),
            artwork: Some(&cover),
            hover_icon: Some(Icon::Play),
            hover_tooltip: Some("Tocar faixa"),
        },
        list_row::ListRow {
            index: Some(1),
            title: "Vento Sul",
            subtitle: Some("Banda Auroral"),
            duration_secs: Some(254),
            artwork: None,
            hover_icon: None,
            hover_tooltip: None,
        },
        list_row::ListRow {
            index: Some(2),
            title: "Pequenos Sinais",
            subtitle: None,
            duration_secs: Some(3_910),
            artwork: None,
            hover_icon: None,
            hover_tooltip: None,
        },
    ];
    for (i, row) in rows.iter().enumerate() {
        let response = list_row::list_row(
            ui,
            theme,
            base.with(("row", i)),
            row.clone(),
            i != 2,
            i == 0,
        );
        if response.clicked() {
            toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
        }
    }
    // Linha desabilitada.
    let disabled_row = list_row::ListRow::plain("Faixa indisponível");
    let _ = list_row::list_row(
        ui,
        theme,
        base.with("row_disabled"),
        disabled_row,
        false,
        false,
    );
}

fn player_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("PlayerBar + Transport + Seek + Volume + QueuePanel")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    let cover = demo_artwork(ui.ctx(), base.with("player_art"), 77);
    let playing: bool = ui
        .ctx()
        .data_mut(|d| d.get_temp(playing_key()))
        .unwrap_or(true);
    let position: u32 = 42;
    let duration: u32 = 214;
    let mut volume: f32 = ui
        .ctx()
        .data_mut(|d| d.get_temp(base.with("volume")))
        .unwrap_or(0.7);

    let state = player_bar::PlayerBarState {
        title: "Cidade do Interior",
        subtitle: "Banda Auroral",
        artwork: Some(&cover),
        playing,
        position_secs: position,
        duration_secs: duration,
        volume,
        queue_open: false,
        enabled: true,
    };
    let response = player_bar::player_bar(ui, theme, base.with("player"), &state);
    if response.play_pause.clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(playing_key(), !playing));
    }
    if let Some(seek) = response.seek {
        ui.ctx().data_mut(|d| {
            d.insert_temp(base.with("seek"), seek);
        });
    }
    if let Some(v) = response.volume {
        volume = v;
        ui.ctx()
            .data_mut(|d| d.insert_temp(base.with("volume"), volume));
    }

    // Painel de fila inline para revisão visual.
    let queue_open: bool = ui
        .ctx()
        .data_mut(|d| d.get_temp(queue_key()))
        .unwrap_or(false);
    ui.horizontal_top(|ui| {
        if button::button(
            ui,
            theme,
            queue_key(),
            button::ButtonVariant::Secondary,
            button::ButtonContent::label(if queue_open {
                "Ocultar fila"
            } else {
                "Mostrar fila"
            }),
            true,
            queue_open,
        )
        .clicked()
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp(queue_key(), !queue_open));
        }
    });
    if queue_open {
        ui.allocate_ui(egui::Vec2::new(420.0, 240.0), |ui| {
            let queue = [
                player_bar::QueueItem {
                    title: "Cidade do Interior",
                    subtitle: "Banda Auroral",
                    duration_secs: 187,
                    active: true,
                },
                player_bar::QueueItem {
                    title: "Vento Sul",
                    subtitle: "Banda Auroral",
                    duration_secs: 254,
                    active: false,
                },
                player_bar::QueueItem {
                    title: "Pequenos Sinais",
                    subtitle: "Banda Auroral",
                    duration_secs: 3_910,
                    active: false,
                },
            ];
            player_bar::queue_panel(ui, theme, base.with("queue"), &queue);
        });
    }
}

fn loading_feedback_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("SkeletonRow (shimmer), ErrorBanner, Toast e Modal")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );
    skeleton_row::skeleton_rows(ui, theme, base.with("skeleton"), 3);
    ui.add_space(spacing::X8);

    error_banner::error_banner(
        ui,
        theme,
        base.with("error"),
        error_banner::BannerKind::Error,
        "Não foi possível conectar ao Apple Music. Tentando de novo…",
        true,
    );
    ui.add_space(spacing::X8);
    let _ = error_banner::error_banner(
        ui,
        theme,
        base.with("warning"),
        error_banner::BannerKind::Warning,
        "Cache em disco quase cheio. Limpe em Configurações › Armazenamento.",
        false,
    );
    ui.add_space(spacing::X8);
    let banner_info = error_banner::error_banner(
        ui,
        theme,
        base.with("info_banner"),
        error_banner::BannerKind::Info,
        "Novo no Sitrus: fila sincronizada entre dispositivos.",
        true,
    );
    if banner_info.is_some_and(|r| r.clicked()) {
        toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
    }

    ui.add_space(spacing::X8);
    ui.horizontal(|ui| {
        if button::button(
            ui,
            theme,
            base.with("toast_trigger"),
            button::ButtonVariant::Secondary,
            button::ButtonContent::label("Toast de sucesso"),
            true,
            false,
        )
        .clicked()
        {
            toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
        }
        if button::button(
            ui,
            theme,
            modal_key(),
            button::ButtonVariant::Primary,
            button::ButtonContent::label("Abrir modal"),
            true,
            false,
        )
        .clicked()
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp::<bool>(modal_key(), true));
        }
    });
    ui.add_space(spacing::X8);
    let _ = anim::HOVER_MS; // tokens de animação disponíveis (120/180/240 ms)
}

/// Seção das páginas de detalhe: DetailHeader, Tabs e TrackTable.
fn detail_pages_section(ui: &mut Ui, theme: &Theme, base: Id) {
    ui.label(
        egui::RichText::new("DetailHeader + Tabs + TrackTable (páginas de artista/álbum/playlist)")
            .font(typography::font_id(type_scale::TypeToken::Footnote))
            .color(theme.label_tertiary),
    );

    // Hero com capa (caso álbum/playlist com artwork).
    let cover = demo_artwork(ui.ctx(), base.with("detail_art"), 17);
    let hero = hero_header::HeroData {
        title: "Midnight Citrus",
        subtitle: Some("Álbum · Sitrus Studio Players · 2026"),
        action_label: None,
        gradient: Some(hero_gradient_of(17)),
    };
    let header = detail_header::DetailHeader::new(hero);
    let output =
        detail_header::detail_header(ui, theme, base.with("detail_header"), &header, Some(&cover));
    if output.back.clicked() {
        toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
    }
    ui.add_space(spacing::X8);

    // Faixa de abas com estados (ativa sublinhada).
    let labels = ["Álbuns", "Músicas"];
    let chosen = tabs::tabs(ui, theme, base.with("tabs"), &labels, 1, true);
    if chosen == Some(0) {
        toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
    }
    ui.add_space(spacing::X8);

    // Tabela de faixas: número, título, duração e hover de tocar.
    let seconds: [u32; 3] = [212, 187, 0];
    let titles = ["Citrus Dawn", "Pulso Noturno", "Interlúdio (Silêncio)"];
    let artists = [
        "Sitrus Studio Players",
        "Sitrus Studio Players",
        "Sitrus Studio Players",
    ];
    let rows: Vec<track_table::TrackRow<'_>> = titles
        .iter()
        .zip(artists.iter())
        .enumerate()
        .map(|(i, (title, artist))| {
            track_table::TrackRow::new(i, title, Some(artist), Some(seconds[i]))
        })
        .collect();
    let clicked = track_table::track_table(ui, theme, base.with("track_table"), &rows, Some(0));
    if clicked.is_some() {
        toast::trigger_toast(ui, toast_key(), toast::DEFAULT_TOAST_DURATION_SECS);
    }
}

/// Textura de capa sintética em cache por id.
fn demo_artwork(ctx: &egui::Context, id: Id, seed: u32) -> egui::TextureHandle {
    let cached: Option<egui::TextureHandle> = ctx.data_mut(|d| d.get_temp(id));
    if let Some(handle) = cached {
        return handle;
    }
    let img = artwork::sample_artwork(seed, 220);
    let handle = artwork::load_texture(ctx, &format!("sitrus_demo_artwork_{seed}"), &img);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    handle
}

/// Gradiente derivado das duas cores dominantes da capa sintética.
fn hero_gradient_of(seed: u32) -> (egui::Color32, egui::Color32) {
    artwork::hero_gradient(&artwork::sample_artwork(seed, 64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A galeria cobre todos os componentes do catálogo (fonte única para a
    /// tela obrigatória de desenvolvimento).
    #[test]
    fn gallery_uses_every_catalog_component() {
        let source = include_str!("gallery.rs");
        for component in [
            "navigation_rail::",
            "hero_header::",
            "detail_header::",
            "tabs::tabs",
            "track_table::track_table",
            "shelf_row::",
            "album_card::",
            "list_row::list_row",
            "section_header::",
            "player_bar::player_bar",
            "segmented_control::",
            "search_field::",
            "empty_state(",
            "skeleton_row::",
            "error_banner::",
            "toast::toast",
            "modal::modal",
            "button::paint_button",
        ] {
            assert!(
                source.contains(component),
                "galeria não menciona {component}"
            );
        }
    }

    /// Gradiente de hero para capa sintética é escuro (texto branco legível).
    #[test]
    fn demo_hero_gradient_is_dark() {
        let (from, to) = hero_gradient_of(11);
        let luminance = |c: egui::Color32| artwork::relative_luminance(c);
        assert!(luminance(from) < 0.20 && luminance(to) < 0.20);
    }
}
