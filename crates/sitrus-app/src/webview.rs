//! Janela de login do player web (feature `webview`).
//!
//! Design em **dois processos**: `EventLoop::run` do tao encerra o processo
//! inteiro ao fechar (`process::exit` interno), portanto a janela em GTK/wry
//! roda como subprocesso isolado (`sitrus-app --login-webview`) e devolve o
//! resultado ao app pelos marcadores em stdout:
//!
//! - `SITRUS_LOGIN_OK` — o media-user-token apareceu na jarra de cookies; o
//!   subprocesso já gravou a sessão em disco (`Session::save`, 0600) e o pai
//!   relê com `Session::load` e entrega ao worker de rede como
//!   `NetCommand::SetSession` (a UI reage ao `NetEvent::SessionReady`).
//! - `SITRUS_LOGIN_CLOSED` — o usuário fechou a janela sem concluir a entrada.
//!
//! Consulta os cookies do player a cada 2 s; nenhum valor de cookie/token é
//! registrado em log ou enviado pelo stdout (só marcadores de status).
//!
//! **Compilação**: este módulo só é construído atrás da feature `webview`
//! (`dep:wry` + `dep:tao`), que exige `webkit2gtk-4.1` instalado no sistema.

use std::io::BufRead;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use crate::net::{NetCommand, NetEvent};
use sitrus_api::Session;

/// URL do player web (origem onde o media-user-token é definido).
const PLAYER_URL: &str = "https://music.apple.com/";
/// Intervalo entre consultas da jarra de cookies.
const COOKIE_POLL: Duration = Duration::from_secs(2);
/// Marcador de sucesso em stdout.
const MARKER_OK: &str = "SITRUS_LOGIN_OK";
/// Marcador de janela fechada sem entrada em stdout.
const MARKER_CLOSED: &str = "SITRUS_LOGIN_CLOSED";

/// Inicia o subprocesso de login e acompanha stdout em thread dedicada.
///
/// - `SITRUS_LOGIN_OK`: a thread relê `Session::load` do disco (o subprocesso
///   gravou a sessão antes do marcador) e entrega a sessão ao worker de rede
///   via `NetCommand::SetSession`; a UI reage ao `NetEvent::SessionReady`
///   correspondente, relendo o estado do disco.
/// - `SITRUS_LOGIN_CLOSED` ou saída sem marcador: publica `NetEvent::LoginError`
///   na UI pelos `events`.
///
/// Não bloqueia a thread da UI. Nenhum valor de cookie/token é registrado em
/// log ou enviado pelo stdout (só marcadores de status).
pub fn spawn_login_process(command_tx: Sender<NetCommand>, event_tx: Sender<NetEvent>) {
    let Ok(executable) = std::env::current_exe() else {
        let _ = event_tx.send(NetEvent::LoginError(
            "não foi possível localizar o executável para abrir o login".to_owned(),
        ));
        return;
    };
    let Ok(child) = Command::new(executable)
        .arg("--login-webview")
        .stdout(std::process::Stdio::piped())
        .spawn()
    else {
        let _ = event_tx.send(NetEvent::LoginError(
            "o subprocesso de login não pôde ser iniciado".to_owned(),
        ));
        return;
    };

    // Thread leitora: converte linhas de stdout em eventos e, no fim,
    // sinaliza fechamento sem entrada quando nenhum marcador chegou.
    let _ = std::thread::Builder::new()
        .name("sitrus-login-watch".to_owned())
        .spawn(move || {
            let mut child = child;
            let mut outcome_sent = false;
            if let Some(stdout) = child.stdout.take() {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    match line.trim() {
                        MARKER_OK => {
                            match Session::load() {
                                Ok(Some(session)) if session.media_user_token.is_some() => {
                                    // Sucesso: a sessão vai ao worker de rede
                                    // (`SetSession` persiste o cliente e inicia
                                    // a carga da biblioteca); nada do token
                                    // trafega em texto puro.
                                    let _ = command_tx.send(NetCommand::SetSession(session));
                                }
                                _ => {
                                    let _ = event_tx.send(NetEvent::LoginError(
                                        "entrada concluída, mas a sessão não \
                                         pôde ser lida do disco"
                                            .to_owned(),
                                    ));
                                }
                            }
                            outcome_sent = true;
                            break;
                        }
                        MARKER_CLOSED => {
                            let _ = event_tx.send(NetEvent::LoginError(
                                "janela de login fechada sem concluir a entrada".to_owned(),
                            ));
                            outcome_sent = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            let _ = child.wait();
            if !outcome_sent {
                let _ = event_tx.send(NetEvent::LoginError(
                    "janela de login fechada antes de concluir a entrada".to_owned(),
                ));
            }
        });
}

/// Executa a janela de login **neste processo** (modo `--login-webview`).
///
/// Bloqueia a thread atual; o tao encerra o processo ao fechar a janela
/// (comportamento da `EventLoop::run` do tao), motivo do subprocesso. Erros
/// antes do loop vão para stderr e o pai vê o stdout fechar sem marcador.
pub fn run_child_login_window() {
    if let Err(message) = run_login_window() {
        eprintln!("sitrus-app: janela de login: {message}");
    }
}

/// Abre player web e consulta cookies até o media-user-token aparecer.
fn run_login_window() -> Result<(), String> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Entrar — Apple Music (Sitrus)")
        .with_inner_size(LogicalSize::new(1120.0_f64, 760.0_f64))
        .build(&event_loop)
        .map_err(|error| format!("janela não pôde ser criada: {error}"))?;

    let builder = WebViewBuilder::new().with_url(PLAYER_URL);
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .ok_or_else(|| "janela sem contêiner gtk".to_owned())?;
        builder
            .build_gtk(vbox)
            .map_err(|error| format!("webview não pôde ser criada: {error}"))?
    };
    let _ = webview.focus();

    let mut completed = false;
    event_loop.run(move |event, _target, control_flow| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Marcador antes do `process::exit` interno do tao.
                println!("{MARKER_CLOSED}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                *control_flow = ControlFlow::Exit;
            }
            Event::NewEvents(StartCause::Init)
            | Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if completed {
                    return;
                }
                *control_flow = ControlFlow::WaitUntil(Instant::now() + COOKIE_POLL);
                if let Ok(cookies) = webview.cookies_for_url(PLAYER_URL) {
                    let header: String = cookies
                        .iter()
                        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    let session = Session::from_pasted_cookies(&header);
                    if session.media_user_token.is_some() {
                        completed = true;
                        // Sessão persistida (0600) antes do marcador: o pai
                        // relê do disco — nada trafega em texto puro.
                        let _ = session.save();
                        println!("{MARKER_OK}");
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    });
}
