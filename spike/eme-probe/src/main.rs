#[cfg(not(feature = "webview"))]
fn main() {
    println!("eme-probe: build with --features webview to run the WebKitGTK probe");
}

#[cfg(feature = "webview")]
fn main() {
    if let Err(error) = webview_probe::run() {
        eprintln!("EME_PROBE_FATAL {error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "webview")]
mod webview_probe {
    use std::{
        cell::RefCell,
        error::Error,
        io::{Read, Write},
        net::TcpListener,
        rc::Rc,
        thread,
        time::Duration,
    };

    use gtk::glib::Propagation;
    use gtk::prelude::*;
    use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix};

    const APPLE_URL: &str = "https://music.apple.com";
    const APPLE_OBSERVE_SECONDS: u32 = 20;
    const OVERALL_TIMEOUT_SECONDS: u32 = 60;

    #[derive(Default)]
    struct ProbeState {
        widevine_result: Option<String>,
        clearkey_result: Option<String>,
        apple_loaded: bool,
        apple_drm_errors: Vec<String>,
        apple_observation_started: bool,
        finished: bool,
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        gtk::init()?;
        let local_url = start_local_probe_server()?;
        let state = Rc::new(RefCell::new(ProbeState::default()));
        let webview_slot: Rc<RefCell<Option<WebView>>> = Rc::new(RefCell::new(None));

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sitrus EME probe");
        window.set_default_size(960, 640);
        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Propagation::Proceed
        });
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&container);

        let handler_state = Rc::clone(&state);
        let handler_webview = Rc::clone(&webview_slot);
        let builder = WebViewBuilder::new()
            .with_url(local_url)
            .with_initialization_script(APPLE_CONSOLE_HOOK)
            .with_ipc_handler(move |request| {
                let message = request.body().as_str();
                if let Some(value) =
                    message.strip_prefix("EME_PROBE keySystem=com.widevine.alpha result=")
                {
                    handler_state.borrow_mut().widevine_result = Some(value.to_owned());
                    println!("{message}");
                } else if let Some(value) =
                    message.strip_prefix("EME_PROBE keySystem=org.w3.clearkey result=")
                {
                    handler_state.borrow_mut().clearkey_result = Some(value.to_owned());
                    println!("{message}");
                } else if message == "EME_PROBE_DONE" {
                    let (widevine, clearkey) = {
                        let current = handler_state.borrow();
                        (
                            current
                                .widevine_result
                                .clone()
                                .unwrap_or_else(|| "MISSING".to_owned()),
                            current
                                .clearkey_result
                                .clone()
                                .unwrap_or_else(|| "MISSING".to_owned()),
                        )
                    };
                    let verdict = if widevine == "SUPPORTED" {
                        "EME_OK"
                    } else {
                        "EME_UNSUPPORTED"
                    };
                    println!("EME_PROBE verdict={verdict} widevine={widevine} clearkey={clearkey}");
                    println!("APPLE_PAGE status=navigating url={APPLE_URL}");

                    let navigation_started = handler_webview
                        .borrow()
                        .as_ref()
                        .is_some_and(|webview| webview.load_url(APPLE_URL).is_ok());
                    if !navigation_started {
                        println!("APPLE_PAGE status=navigation-error");
                    }

                    let should_start_timer = {
                        let mut current = handler_state.borrow_mut();
                        if current.apple_observation_started {
                            false
                        } else {
                            current.apple_observation_started = true;
                            true
                        }
                    };
                    if should_start_timer {
                        let timer_state = Rc::clone(&handler_state);
                        gtk::glib::timeout_add_seconds_local(APPLE_OBSERVE_SECONDS, move || {
                            finish_observation(&timer_state);
                            gtk::glib::ControlFlow::Break
                        });
                    }
                } else if message == "APPLE_PAGE_LOADED" {
                    let mut current = handler_state.borrow_mut();
                    if !current.apple_loaded {
                        current.apple_loaded = true;
                        println!("APPLE_PAGE status=loaded");
                    }
                } else if message.starts_with("APPLE_DRM_CONSOLE ") {
                    handler_state
                        .borrow_mut()
                        .apple_drm_errors
                        .push(message.to_owned());
                    println!("{message}");
                } else if message == "EME_PROBE_FATAL page_script_error" {
                    println!("EME_PROBE_ERROR page_script_error");
                }
            });

        let webview = builder.build_gtk(&container)?;
        *webview_slot.borrow_mut() = Some(webview);
        window.show_all();

        let timeout_state = Rc::clone(&state);
        gtk::glib::timeout_add_seconds_local(OVERALL_TIMEOUT_SECONDS, move || {
            let mut current = timeout_state.borrow_mut();
            if !current.finished {
                eprintln!("EME_PROBE_ERROR overall_timeout");
                current.finished = true;
                gtk::main_quit();
            }
            gtk::glib::ControlFlow::Break
        });

        gtk::main();
        let final_state = state.borrow();
        if final_state.widevine_result.is_none()
            || final_state.clearkey_result.is_none()
            || !final_state.finished
        {
            return Err("probe did not complete before the timeout or window close".into());
        }
        Ok(())
    }

    fn finish_observation(state: &Rc<RefCell<ProbeState>>) {
        let mut current = state.borrow_mut();
        if current.finished {
            return;
        }
        if !current.apple_drm_errors.is_empty() {
            println!(
                "APPLE_DRM_CONSOLE summary=error_observed count={}",
                current.apple_drm_errors.len()
            );
        } else if current.apple_loaded {
            println!("APPLE_DRM_CONSOLE summary=none observed_for_s={APPLE_OBSERVE_SECONDS}");
        } else {
            println!(
                "APPLE_DRM_CONSOLE summary=none page_loaded=false observed_for_s={APPLE_OBSERVE_SECONDS}"
            );
        }
        current.finished = true;
        println!("EME_PROBE_COMPLETE");
        gtk::main_quit();
    }

    fn start_local_probe_server() -> Result<String, Box<dyn Error>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let address = listener.local_addr()?;
        let page = include_bytes!("../probe.html").to_vec();
        thread::Builder::new()
            .name("eme-probe-http".to_owned())
            .spawn(move || {
                for incoming in listener.incoming() {
                    let Ok(mut stream) = incoming else {
                        continue;
                    };
                    let mut request = [0_u8; 2048];
                    let _ = stream.read(&mut request);
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        page.len()
                    );
                    if stream.write_all(header.as_bytes()).is_ok() {
                        let _ = stream.write_all(&page);
                        let _ = stream.flush();
                    }
                }
            })?;
        thread::sleep(Duration::from_millis(20));
        Ok(format!("http://{address}/"))
    }

    const APPLE_CONSOLE_HOOK: &str = r#"(() => {
      if (window.__sitrusEmeProbeHooked) return;
      window.__sitrusEmeProbeHooked = true;
      const isAppleMusic = location.hostname === "music.apple.com";
      const relevant = /widevine|drm|eme|encrypted media|key system|keysystem|media key/i;
      const categories = (text) => {
        const matches = text.match(/widevine|drm|eme|encrypted media|key system|keysystem|media key/ig) || [];
        return [...new Set(matches.map((item) => item.toLowerCase().replaceAll(" ", "_")))].join(",");
      };
      const report = (source, text) => {
        if (!isAppleMusic || !relevant.test(text)) return;
        window.ipc.postMessage(`APPLE_DRM_CONSOLE source=${source} matches=${categories(text)}`);
      };
      for (const method of ["error", "warn"]) {
        const original = console[method].bind(console);
        console[method] = (...args) => {
          report(`console.${method}`, args.map(String).join(" "));
          return original(...args);
        };
      }
      window.addEventListener("error", (event) => report("window.error", String(event.message || "")));
      window.addEventListener("unhandledrejection", (event) => report("unhandledrejection", String(event.reason || "")));
      window.addEventListener("load", () => {
        if (isAppleMusic) window.ipc.postMessage("APPLE_PAGE_LOADED");
      }, { once: true });
    })();"#;
}
