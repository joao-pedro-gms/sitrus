//! Ponto de entrada do aplicativo Sitrus.
//!
//! Sem argumentos abre a janela principal (eframe). `--gallery` abre a janela
//! da galeria de componentes de `sitrus-ui`; `--login-manual` abre a janela
//! principal direto na tela de login manual (caminho de cookies colados, que
//! funciona mesmo sem a feature `webview`). `--login-webview` é um modo
//! interno reservado ao subprocesso de login por webview (feature `webview`
//! exige `webkit2gtk-4.1` no sistema).

use std::process::ExitCode;

use eframe::egui;

mod app;
mod login;
mod model;
mod net;
mod player;
#[cfg(feature = "webview")]
mod webview;

use crate::app::{MIN_WINDOW_SIZE, WINDOW_SIZE, WINDOW_TITLE};

/// Ajuda curta exibida em `--help` e em erros de uso.
const USAGE: &str = "\
Uso: sitrus-app [opção]

Sem opção abre a janela principal do Sitrus.

Opções:
  --version        Exibe a versão do aplicativo e encerra (-V).
  --gallery        Abre a janela da galeria de componentes (sitrus-ui).
  --login-manual   Abre a janela principal direto na tela de login manual.
  --help           Exibe esta ajuda (-h).
";

type NativeResult = Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>>;

/// Abre a janela principal; `manual_login` entra direto na tela de login.
fn run_main_window(manual_login: bool) -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(MIN_WINDOW_SIZE),
        centered: true,
        ..eframe::NativeOptions::default()
    };
    let creator = move |creation: &eframe::CreationContext<'_>| -> NativeResult {
        // Tema/zoom do usuário são aplicados dentro de App::new.
        let window: Box<dyn eframe::App> =
            Box::new(app::App::new(&creation.egui_ctx, manual_login));
        Ok(window)
    };
    match eframe::run_native(WINDOW_TITLE, options, Box::new(creator)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sitrus-app: a janela não pôde ser aberta: {error:?}");
            ExitCode::FAILURE
        }
    }
}

/// Abre a janela da galeria de componentes (sitrus-ui).
fn run_gallery() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sitrus — Galeria do design system")
            .with_inner_size(egui::vec2(1200.0, 900.0)),
        ..eframe::NativeOptions::default()
    };
    let creator = |_: &eframe::CreationContext<'_>| -> NativeResult {
        Ok(Box::new(sitrus_ui::SitrusApp::new().with_gallery()))
    };
    match eframe::run_native("Sitrus — Galeria", options, Box::new(creator)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sitrus-app: a galeria não pôde ser aberta: {error:?}");
            ExitCode::FAILURE
        }
    }
}

/// Modo interno do subprocesso de login por webview (feature `webview`).
#[cfg(feature = "webview")]
fn run_login_webview_child() -> ExitCode {
    webview::run_child_login_window();
    // Alcançável apenas quando a janela não abriu (erro antes do loop).
    ExitCode::FAILURE
}

#[cfg(not(feature = "webview"))]
fn run_login_webview_child() -> ExitCode {
    eprintln!("sitrus-app: --login-webview exige a feature `webview`");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => run_main_window(false),
        [arg] if arg == "--version" || arg == "-V" => {
            println!("sitrus-app {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [arg] if arg == "--gallery" => run_gallery(),
        [arg] if arg == "--login-manual" => run_main_window(true),
        [arg] if arg == "--login-webview" => run_login_webview_child(),
        [arg] if arg == "--help" || arg == "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            let shown = if other.is_empty() {
                "<nenhum>".to_owned()
            } else {
                other.join(" ")
            };
            eprintln!("{USAGE}");
            eprintln!("argumento(s) não reconhecido(s): {shown}");
            ExitCode::from(2)
        }
    }
}
