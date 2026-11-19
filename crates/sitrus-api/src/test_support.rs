//! Apoio exclusivo aos testes da crate (compilado apenas com `cfg(test)`).
//!
//! Oferece um servidor HTTP de loopback que responde com corpos fixos e
//! registra as requisições — para afirmar caminho, query e cabeçalhos sem
//! rede externa — e um [`TokenProvider`] com bearer pré-carregado em cache.
//! Todas as conexões vão exclusivamente para `127.0.0.1:0`; nenhuma conexão
//! sai da máquina e nenhuma dependência nova é necessária.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use url::Url;

use crate::token::{DeveloperToken, TokenProvider, now_unix};

/// Requisição capturada pelo mock: destino e cabeçalhos como chegaram.
#[derive(Debug, Clone)]
pub(crate) struct MockRequest {
    /// Linha de destino (`/caminho?query`).
    pub target: String,
    headers: Vec<(String, String)>,
}

impl MockRequest {
    /// Valor do cabeçalho `name` (comparação sem caixa), se enviado.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Servidor HTTP de loopback que responde com os pares `(status, corpo)` em
/// ordem e registra as requisições recebidas. Cada resposta sai com
/// `Connection: close`, forçando uma nova conexão para cada chamada do
/// cliente — a ordem das respostas acompanha a ordem das chamadas.
pub(crate) struct MockServer {
    /// URL base para o cliente apontar: `http://127.0.0.1:{porta}/`.
    pub base: Url,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    _thread: thread::JoinHandle<()>,
}

impl MockServer {
    /// Sobe o mock em porta efêmera montando as respostas com acesso à base
    /// do próprio servidor (útil para cursores `next` que apontam para si).
    pub fn start_with<F>(responses: F) -> Self
    where
        F: FnOnce(&Url) -> Vec<(u16, String)>,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind no loopback");
        let base = Url::parse(&format!(
            "http://{}/",
            listener.local_addr().expect("addr local do mock")
        ))
        .expect("URL base do mock");
        let responses = responses(&base);
        Self::serve(listener, base, responses)
    }

    /// Sobe o mock em porta efêmera com as respostas fixadas em ordem.
    pub fn start(responses: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind no loopback");
        let base = Url::parse(&format!(
            "http://{}/",
            listener.local_addr().expect("addr local do mock")
        ))
        .expect("URL base do mock");
        Self::serve(listener, base, responses)
    }

    /// Loop de atendimento: responde cada conexão com o próximo par
    /// `(status, corpo)` fixado e registra a requisição recebida.
    fn serve(listener: TcpListener, base: Url, responses: Vec<(u16, String)>) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let registered = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let head = read_request_head(&mut stream);
                registered
                    .lock()
                    .expect("lock das requisições do mock")
                    .push(MockRequest {
                        target: request_target(&head),
                        headers: request_headers(&head),
                    });
                let response = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    reason_phrase(status),
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            base,
            requests,
            _thread: thread,
        }
    }

    /// Requisição registrada na posição `index` (0-baseada) ou panic com
    /// mensagem clara quando a requisição não chegou ao mock.
    pub fn request(&self, index: usize) -> MockRequest {
        self.requests
            .lock()
            .expect("lock das requisições do mock")
            .get(index)
            .cloned()
            .unwrap_or_else(|| panic!("requisição {index} não chegou ao mock"))
    }

    /// Total de requisições registradas até agora.
    pub fn request_count(&self) -> usize {
        self.requests
            .lock()
            .expect("lock das requisições do mock")
            .len()
    }
}

/// Lê do socket até o fim do cabeçalho HTTP (`\r\n\r\n`); os GETs dos testes
/// não têm corpo relevante.
fn read_request_head(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(size) => size,
        };
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() > 64 * 1024 {
            break;
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// Primeiro caminho da linha de requisição: `GET /caminho?query HTTP/1.1`.
fn request_target(head: &str) -> String {
    head.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned()
}

/// Pares `(nome, valor)` dos cabeçalhos da requisição, sem a linha inicial.
fn request_headers(head: &str) -> Vec<(String, String)> {
    head.lines()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Linha de motivo dos status usados nos testes (o cliente não valida a
/// frase; os nomes existem para legibilidade no traço de rede).
fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Mock Response",
    }
}

/// [`TokenProvider`] com o bearer já em cache: os testes exercitam o envio
/// autenticado sem tocar na rede para descobrir token.
pub(crate) async fn seeded_provider(bearer: &str) -> TokenProvider {
    let provider = TokenProvider::new(reqwest::Client::new());
    provider
        .seed_cache(DeveloperToken {
            token: bearer.to_owned(),
            expires_at: now_unix() + 3600,
        })
        .await;
    provider
}
