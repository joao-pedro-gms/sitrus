//! Protocolo JSON-RPC 2.0 do sidecar e configuração do cliente.
//!
//! Transporte: WebSocket (`ws://127.0.0.1:<porta efêmera>`), um cliente por
//! execução, token de sessão em `initialize` como primeira mensagem (o token
//! nunca vai em URL, argv ou log).
//!
//! Formato das mensagens no fio:
//!
//! ```text
//! cliente → sidecar (requests):
//!   {"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocol":1,"token":"..."}}
//!   {"jsonrpc":"2.0","id":1,"method":"load","params":{"items":[{"id":"1","durationMs":100000}]}}
//!   {"jsonrpc":"2.0","id":2,"method":"play","params":{}}
//!   {"jsonrpc":"2.0","id":3,"method":"seek","params":{"positionMs":30000}}
//!   {"jsonrpc":"2.0","id":4,"method":"volume","params":{"volume":0.5}}
//!   {"jsonrpc":"2.0","id":7,"method":"session_probe","params":{}}
//! sidecar → cliente (resposta + notificações):
//!   {"jsonrpc":"2.0","id":2,"result":null}
//!   {"jsonrpc":"2.0","id":7,"result":{"cookies":[{"name":"…","value":"…","domain":".apple.com"}],"mediaUserToken":"…"}}
//!   {"jsonrpc":"2.0","method":"event","params":{"type":"state","value":"playing"}}
//!   {"jsonrpc":"2.0","method":"event","params":{"type":"position","ms":12000}}
//! ```

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::{EngineEvent, EngineState, QueueItem};

/// Versão do protocolo trocada no `initialize`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Prefixo da linha única que o sidecar imprime em stdout ao subir.
pub const READY_LINE_PREFIX: &str = "SITRUS_SIDECAR_READY";

/// Códigos JSON-RPC usados pelo sidecar.
pub const RPC_INVALID_TOKEN: i32 = -32001;
pub const RPC_PAGE_CONTROL_UNAVAILABLE: i32 = -32002;
pub const RPC_PLAYBACK: i32 = -32000;
pub const RPC_METHOD_NOT_FOUND: i32 = -32601;
pub const RPC_INVALID_PARAMS: i32 = -32602;
pub const RPC_INTERNAL: i32 = -32603;

/// Estado do engine na representação do fio (`snake_case`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireState {
    /// Sem fila carregada ou reprodução interrompida.
    Stopped,
    /// Preparando a fonte.
    Loading,
    /// Reprodução em andamento.
    Playing,
    /// Reprodução pausada.
    Paused,
}

impl From<EngineState> for WireState {
    fn from(state: EngineState) -> Self {
        match state {
            EngineState::Stopped => Self::Stopped,
            EngineState::Loading => Self::Loading,
            EngineState::Playing => Self::Playing,
            EngineState::Paused => Self::Paused,
        }
    }
}

impl From<WireState> for EngineState {
    fn from(state: WireState) -> Self {
        match state {
            WireState::Stopped => Self::Stopped,
            WireState::Loading => Self::Loading,
            WireState::Playing => Self::Playing,
            WireState::Paused => Self::Paused,
        }
    }
}

/// Item de fila na representação do fio (campos `camelCase`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireQueueItem {
    /// Identificador opaco da faixa.
    pub id: String,
    /// Título de exibição opcional.
    pub title: Option<String>,
    /// Duração conhecida em milissegundos.
    pub duration_ms: Option<u64>,
    /// URL de mídia direta opcional.
    pub url: Option<String>,
}

impl From<&QueueItem> for WireQueueItem {
    fn from(item: &QueueItem) -> Self {
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            duration_ms: item.duration_ms,
            url: item.url.clone(),
        }
    }
}

/// Conteúdo de uma notificação `event` do sidecar.
///
/// Enum com tag interna: `{"type":"state","value":"playing"}`,
/// `{"type":"position","ms":10}`, `{"type":"ended"}` etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum EventParams {
    /// Estado mudou.
    State {
        /// Novo estado.
        value: WireState,
    },
    /// Posição atual em milissegundos.
    Position {
        /// Posição em milissegundos.
        ms: u64,
    },
    /// Duração conhecida em milissegundos.
    Duration {
        /// Duração em milissegundos.
        ms: u64,
    },
    /// Faixa terminou.
    Ended {},
    /// Falha reportada pelo lado do sidecar.
    Error {
        /// Mensagem sem segredos.
        message: String,
    },
}

impl From<&EngineEvent> for EventParams {
    fn from(event: &EngineEvent) -> Self {
        match event {
            EngineEvent::StateChanged(state) => Self::State {
                value: WireState::from(*state),
            },
            EngineEvent::Position(d) => Self::Position {
                ms: u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            },
            EngineEvent::Duration(d) => Self::Duration {
                ms: u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            },
            EngineEvent::Ended => Self::Ended {},
            EngineEvent::Error(message) => Self::Error {
                message: message.clone(),
            },
            // CommandFailed nunca vem do fio (o actor resolve os prazos no
            // lado Rust); na serialização de eventos de teste, degrada para a
            // mensagem do erro tipado, sem segredos (Display do EngineError).
            EngineEvent::CommandFailed(error) => Self::Error {
                message: error.to_string(),
            },
        }
    }
}

impl TryFrom<EventParams> for EngineEvent {
    type Error = EngineError;

    fn try_from(params: EventParams) -> Result<Self, EngineError> {
        Ok(match params {
            EventParams::State { value } => Self::StateChanged(EngineState::from(value)),
            EventParams::Position { ms } => Self::Position(Duration::from_millis(ms)),
            EventParams::Duration { ms } => Self::Duration(Duration::from_millis(ms)),
            EventParams::Ended {} => Self::Ended,
            EventParams::Error { message } => Self::Error(message),
        })
    }
}

/// Chamada RPC do cliente ao sidecar (tag `method` + `params`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum MethodCall {
    /// Primeira mensagem da conexão: autentica a sessão.
    Initialize {
        /// Versão do protocolo esperada pelo cliente.
        protocol: u32,
        /// Token de sessão gerado por execução.
        token: String,
    },
    /// Substitui a fila inteira.
    Load {
        /// Itens, na ordem de reprodução.
        items: Vec<WireQueueItem>,
    },
    /// Inicia ou retoma a reprodução.
    Play {},
    /// Pausa a reprodução.
    Pause {},
    /// Posiciona o cursor da faixa corrente.
    Seek {
        /// Posição alvo em milissegundos.
        position_ms: u64,
    },
    /// Ajusta o volume (0.0..=1.0).
    Volume {
        /// Volume alvo.
        volume: f32,
    },
    /// Encerra a sessão com elegância.
    Shutdown {},
    /// Diagnóstico: devolve a sessão observada pelo lado do sidecar — o echo
    /// do envelope recebido (modo mock) ou o `session.defaultSession.cookies.get`
    /// observado no processo do Electron (modo electron). Os valores trafegam
    /// só neste canal local autenticado; nenhum log os imprime.
    SessionProbe {},
}

/// Envelope JSON-RPC de request (serialize-only: o cliente nunca recebe isso).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RpcRequestFrame<'a> {
    /// Versão do JSON-RPC.
    pub jsonrpc: &'a str,
    /// Identificador numérico do request.
    pub id: u64,
    /// Método e parâmetros (tag + content do [`MethodCall`]).
    #[serde(flatten)]
    pub call: &'a MethodCall,
}

/// Objeto de erro JSON-RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcErrorObject {
    /// Código da falha (ver constantes `RPC_*`).
    pub code: i32,
    /// Mensagem legível, sem segredos.
    pub message: String,
}

/// Envelope JSON-RPC de resposta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcResponseFrame {
    /// Versão do JSON-RPC.
    pub jsonrpc: String,
    /// Identificador do request respondido.
    pub id: u64,
    /// Resultado (`null` nos comandos sem payload).
    pub result: Option<serde_json::Value>,
    /// Erro, quando a chamada falhou do lado do sidecar.
    pub error: Option<RpcErrorObject>,
}

/// Envelope JSON-RPC de notificação (sem `id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcNotification {
    /// Versão do JSON-RPC.
    pub jsonrpc: String,
    /// Nome do método; só `event` é aceito.
    pub method: String,
    /// Payload tipado do evento.
    pub params: EventParams,
}

/// Mensagem do sidecar após o parse do frame bruto.
#[derive(Debug, Clone, PartialEq)]
pub enum IncomingMessage {
    /// Resposta a um request (com `id`).
    Response(RpcResponseFrame),
    /// Notificação de evento push.
    Event(EventParams),
}

impl IncomingMessage {
    /// Interpreta um `serde_json::Value` vindo do WebSocket.
    ///
    /// Resposta tem `id` + (`result`|`error`); notificação tem `method`
    /// igual a `event`. Qualquer outra forma vira
    /// [`EngineError::InvalidMessage`].
    pub fn parse(value: serde_json::Value) -> Result<Self, EngineError> {
        let object = match value {
            serde_json::Value::Object(object) => object,
            _ => {
                return Err(EngineError::InvalidMessage {
                    reason: "frame is not a JSON object".to_owned(),
                });
            }
        };

        let has_id = object.contains_key("id");
        let has_result_or_error = object.contains_key("result") || object.contains_key("error");
        if has_id && has_result_or_error {
            let parsed: RpcResponseFrame =
                serde_json::from_value(serde_json::Value::Object(object)).map_err(|error| {
                    EngineError::InvalidMessage {
                        reason: format!("invalid response frame: {error}"),
                    }
                })?;
            return Ok(Self::Response(parsed));
        }

        if object.get("method").and_then(serde_json::Value::as_str) == Some("event") {
            let parsed: RpcNotification = serde_json::from_value(serde_json::Value::Object(object))
                .map_err(|error| EngineError::InvalidMessage {
                    reason: format!("invalid event notification: {error}"),
                })?;
            return Ok(Self::Event(parsed.params));
        }

        Err(EngineError::InvalidMessage {
            reason: "frame is neither a response nor an event notification".to_owned(),
        })
    }
}

/// Cookie transportado ao sidecar para injeção no perfil do Electron antes do
/// `loadURL` da página.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarCookie {
    /// Nome do cookie (ex.: `it.vt`).
    pub name: String,
    /// Valor do cookie. Segredo: via stdin (Rust→sidecar) e arquivo temporário
    /// 0600 (sidecar→Electron); nunca em argv, env, URL ou log.
    pub value: String,
    /// Domínio alvo (ex.: `.apple.com`); ausente → o Electron usa `.apple.com`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

/// Sessão Apple autenticada transportada ao sidecar: cookies do player web
/// mais `media-user-token`. O transporte é o stdin do sidecar (uma linha JSON) ou
/// um arquivo temporário 0600 no diretório de runtime — nunca argv, env, URL
/// ou log. Construção de referência a partir de `sitrus-api::Session`:
/// cada pair `nome→valor` de `session.cookies` vira [`SidecarCookie`] com
/// `domain: None` e `session.media_user_token` vira `media_user_token`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarSession {
    /// Media User Token do player web (cabeçalho `Media-User-Token`).
    pub media_user_token: Option<String>,
    /// Cookies capturados do player web, na ordem de injeção.
    pub cookies: Vec<SidecarCookie>,
}

impl SidecarSession {
    /// Versão do envelope transportado no stdin do sidecar.
    pub const ENVELOPE_VERSION: u32 = 1;

    const MAX_COOKIES: usize = 64;
    const MAX_NAME: usize = 256;
    const MAX_VALUE: usize = 4096;
    const MAX_TOKEN: usize = 2048;

    /// Valida a sessão antes do spawn: carrega ao menos um cookie ou
    /// `media_user_token`, e nomes/valores não vazios dentro dos limites.
    ///
    /// # Erros
    /// [`EngineError::SessionInvalid`] com o motivo da rejeição.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.cookies.is_empty() && self.media_user_token.is_none() {
            return Err(EngineError::SessionInvalid {
                reason: "session carries neither cookies nor a media user token".to_owned(),
            });
        }
        if let Some(token) = &self.media_user_token {
            if token.trim().is_empty() {
                return Err(EngineError::SessionInvalid {
                    reason: "media user token must not be empty".to_owned(),
                });
            }
            if token.len() > Self::MAX_TOKEN {
                return Err(EngineError::SessionInvalid {
                    reason: "media user token exceeds 2048 bytes".to_owned(),
                });
            }
        }
        if self.cookies.len() > Self::MAX_COOKIES {
            return Err(EngineError::SessionInvalid {
                reason: format!("session carries more than {} cookies", Self::MAX_COOKIES),
            });
        }
        for cookie in &self.cookies {
            if cookie.name.trim().is_empty() {
                return Err(EngineError::SessionInvalid {
                    reason: "cookie name must not be empty".to_owned(),
                });
            }
            if cookie.name.len() > Self::MAX_NAME {
                return Err(EngineError::SessionInvalid {
                    reason: format!(
                        "cookie name exceeds {} bytes: {}",
                        Self::MAX_NAME,
                        cookie.name
                    ),
                });
            }
            if cookie.value.is_empty() {
                return Err(EngineError::SessionInvalid {
                    reason: format!("cookie value must not be empty for {}", cookie.name),
                });
            }
            if cookie.value.len() > Self::MAX_VALUE {
                return Err(EngineError::SessionInvalid {
                    reason: format!(
                        "cookie value exceeds {} bytes for {}",
                        Self::MAX_VALUE,
                        cookie.name
                    ),
                });
            }
            if let Some(domain) = &cookie.domain
                && domain.trim().is_empty()
            {
                return Err(EngineError::SessionInvalid {
                    reason: format!("cookie {} carries an empty domain", cookie.name),
                });
            }
        }
        Ok(())
    }

    /// Linha única do envelope JSON para o stdin do sidecar
    /// (`{"v":1,"mediaUserToken":"…","cookies":[{"name":"…","value":"…"}]}\n`).
    #[must_use]
    pub fn envelope(&self) -> String {
        let frame = SessionEnvelopeFrame {
            v: Self::ENVELOPE_VERSION,
            media_user_token: self.media_user_token.as_deref(),
            cookies: &self.cookies,
        };
        let mut line = serde_json::to_string(&frame)
            .unwrap_or_else(|_| format!(r#"{{"v":{},"cookies":[]}}"#, Self::ENVELOPE_VERSION));
        line.push('\n');
        line
    }
}

/// Envelope serialize-only da sessão (uma linha no stdin do sidecar). O lado
/// Node valida `v == 1` e normaliza cookie a cookie; o conteúdo é segredo e
/// não reaparece em argv, env, URL ou log.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionEnvelopeFrame<'a> {
    v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_user_token: Option<&'a str>,
    cookies: &'a [SidecarCookie],
}

/// Cookie observado pelo lado do sidecar: no modo mock é o echo do envelope
/// recebido; no modo electron é um item do `session.defaultSession.cookies.get({url:
/// 'https://music.apple.com/'})` — campos extra do Electron são ignorados no
/// parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieObservation {
    /// Nome do cookie observado.
    pub name: String,
    /// Valor observado (tráfego restrito ao WS local autenticado).
    pub value: String,
    /// Domínio quando informado pelo observador.
    pub domain: Option<String>,
    /// Path quando informado pelo observador.
    pub path: Option<String>,
}

/// Sessão observada no sidecar, devolvida pelo método `session_probe`
/// (somente diagnóstico/teste). Os valores saem do sidecar apenas por este
/// canal; nenhum log os imprime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProbe {
    /// Cookies injetados/observados.
    pub cookies: Vec<CookieObservation>,
    /// Media user token recebido pelo sidecar.
    pub media_user_token: Option<String>,
}

/// Porta efêmera anunciada na linha de pronto do sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarReady {
    /// Porta TCP efêmera em 127.0.0.1.
    pub port: u16,
}

/// Faz o parse da linha `SITRUS_SIDECAR_READY {"port": ...}`.
pub fn parse_ready_line(line: &str) -> Result<SidecarReady, EngineError> {
    let payload = line
        .strip_prefix(READY_LINE_PREFIX)
        .map(str::trim)
        .ok_or_else(|| EngineError::InvalidMessage {
            reason: "stdout line is not the sidecar readiness marker".to_owned(),
        })?;
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| EngineError::InvalidMessage {
            reason: format!("readiness payload is not JSON: {error}"),
        })?;
    let port = value
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .ok_or(EngineError::InvalidMessage {
            reason: "readiness payload is missing an integer port".to_owned(),
        })?;
    let port = u16::try_from(port).map_err(|_| EngineError::InvalidMessage {
        reason: format!("readiness port {port} is out of range"),
    })?;
    Ok(SidecarReady { port })
}

/// Modo de operação do sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidecarMode {
    /// Sidecar completo: Electron castlabs com CDM Widevine (produção).
    #[default]
    Electron,
    /// Reprodução simulada em Node puro (testes e CI sem display).
    Mock,
}

/// Página carregada pelo renderer do sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidecarPage {
    /// Apple Music (produção).
    #[default]
    AppleMusic,
    /// Página mock local (prova de protocolo sem sessão Apple).
    Mock,
    /// Página mock que imita a API MusicKit da Apple: exercita o adapter
    /// in-page (`engine/lib/page-adapter.js`) com a página como fonte real
    /// de eventos, sem sessão Apple e sem rede.
    AppleMock,
}

/// Configuração de spawn do sidecar.
#[derive(Clone)]
pub struct SidecarConfig {
    /// Binário do Node (`node` resolvida no `PATH` por padrão).
    pub node_path: String,
    /// Diretório do sidecar (`engine/` do repo ou do pacote instalado).
    pub engine_dir: PathBuf,
    /// Modo de operação ([`SidecarMode::Electron`] por padrão).
    pub mode: SidecarMode,
    /// Página carregada pelo renderer.
    pub page: SidecarPage,
    /// Prazo de cada RPC (padrão 5 s).
    pub rpc_timeout: Duration,
    /// Prazo do handshake WebSocket + `initialize` (padrão 5 s).
    pub handshake_timeout: Duration,
    /// Prazo para a linha de pronto (padrão 10 s).
    pub spawn_timeout: Duration,
    /// Intervalo do tick do mock em ms (opcional; testes usam valores baixos).
    pub tick_ms: Option<u64>,
    /// Argumentos extras do Electron (por exemplo, `--no-sandbox` em CI).
    pub electron_args: Vec<String>,
    /// Mostra a janela do Electron (padrão: oculta).
    pub show_window: bool,
    /// `userData` do Electron (para isolar CDM em testes).
    pub user_data_dir: Option<PathBuf>,
    /// Sessão Apple (cookies + media-user-token) injetada no perfil do
    /// Electron antes do `loadURL`. `None` mantém o comportamento anterior
    /// (página não autenticada). O transporte é o stdin do sidecar (envelope
    /// JSON de uma linha) e depois um arquivo temporário 0600 para o
    /// Electron — nunca argv, env, URL ou log.
    pub session: Option<SidecarSession>,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            node_path: "node".to_owned(),
            engine_dir: PathBuf::new(),
            mode: SidecarMode::Electron,
            page: SidecarPage::AppleMusic,
            rpc_timeout: Duration::from_secs(5),
            handshake_timeout: Duration::from_secs(5),
            spawn_timeout: Duration::from_secs(10),
            tick_ms: None,
            electron_args: Vec::new(),
            show_window: false,
            user_data_dir: None,
            session: None,
        }
    }
}

impl fmt::Debug for SidecarConfig {
    /// Debug sem segredos: a sessão ([`SidecarSession`], em
    /// `SidecarConfig::session`) é redigida — valor de cookie e
    /// media-user-token nunca aparecem em log/depuração.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SidecarConfig")
            .field("node_path", &self.node_path)
            .field("engine_dir", &self.engine_dir)
            .field("mode", &self.mode)
            .field("page", &self.page)
            .field("rpc_timeout", &self.rpc_timeout)
            .field("handshake_timeout", &self.handshake_timeout)
            .field("spawn_timeout", &self.spawn_timeout)
            .field("tick_ms", &self.tick_ms)
            .field("electron_args", &self.electron_args)
            .field("show_window", &self.show_window)
            .field("user_data_dir", &self.user_data_dir)
            .finish()
    }
}

impl SidecarConfig {
    /// Caminho do script de entrada do sidecar.
    #[must_use]
    pub fn entry_script(&self) -> PathBuf {
        self.engine_dir.join("sidecar.js")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_call_play_serializes_empty_params() {
        let frame = serde_json::to_value(RpcRequestFrame {
            jsonrpc: "2.0",
            id: 2,
            call: &MethodCall::Play {},
        })
        .unwrap();
        assert_eq!(
            frame,
            serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "play", "params": {}})
        );
    }

    #[test]
    fn method_call_session_probe_uses_the_wire_snake_case_name() {
        let frame = serde_json::to_value(RpcRequestFrame {
            jsonrpc: "2.0",
            id: 9,
            call: &MethodCall::SessionProbe {},
        })
        .unwrap();
        assert_eq!(
            frame,
            serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "session_probe", "params": {}})
        );
    }

    /// O probe parseia os dois formatos observados: o echo do modo mock
    /// (envelope) e o `cookies.get` do Electron (campos extras ignorados).
    #[test]
    fn session_probe_parses_the_observed_payload_shapes() {
        let mock_echo = serde_json::json!({
            "cookies": [{"name": "it.vt", "value": "cookie-value", "domain": ".apple.com"}],
            "mediaUserToken": "media-token"
        });
        let probe: SessionProbe = serde_json::from_value(mock_echo).unwrap();
        assert_eq!(probe.cookies.len(), 1);
        assert_eq!(probe.cookies[0].name, "it.vt");
        assert_eq!(probe.cookies[0].value, "cookie-value");
        assert_eq!(probe.cookies[0].domain.as_deref(), Some(".apple.com"));
        assert_eq!(probe.media_user_token.as_deref(), Some("media-token"));

        let electron_observation = serde_json::json!({
            "cookies": [{
                "name": "citm",
                "value": "cookie-value",
                "domain": ".apple.com",
                "path": "/",
                "secure": true,
                "httpOnly": false,
                "hostOnly": false,
                "session": true,
                "sameSite": "unspecified"
            }],
            "mediaUserToken": null
        });
        let probe: SessionProbe = serde_json::from_value(electron_observation).unwrap();
        assert_eq!(probe.cookies[0].path.as_deref(), Some("/"));
        assert!(probe.media_user_token.is_none());
    }

    /// Envelope exato: versão, token opcional e cookies com skip de campo
    /// ausente — a linha transportada no stdin é previsível byte a byte.
    #[test]
    fn session_envelope_is_a_predictable_single_line() {
        let session = SidecarSession {
            media_user_token: Some("media-token".to_owned()),
            cookies: vec![SidecarCookie {
                name: "it.vt".to_owned(),
                value: "cookie-value".to_owned(),
                domain: None,
            }],
        };
        assert_eq!(
            session.envelope(),
            "{\"v\":1,\"mediaUserToken\":\"media-token\",\"cookies\":[{\"name\":\"it.vt\",\"value\":\"cookie-value\"}]}\n"
        );
    }

    /// Envelope vazio e cookies malformados são rejeitados antes do spawn.
    #[test]
    fn sidecar_session_validation_is_input_local() {
        let empty = SidecarSession::default();
        assert!(matches!(
            empty.validate(),
            Err(EngineError::SessionInvalid { .. })
        ));

        let valueless = SidecarSession {
            media_user_token: None,
            cookies: vec![SidecarCookie {
                name: "geo".to_owned(),
                value: String::new(),
                domain: None,
            }],
        };
        assert!(matches!(
            valueless.validate(),
            Err(EngineError::SessionInvalid { .. })
        ));

        let only_token = SidecarSession {
            media_user_token: Some("media-token".to_owned()),
            cookies: Vec::new(),
        };
        assert!(only_token.validate().is_ok());
    }

    /// O Debug da configuração é redigido: nenhum valor de cookie nem
    /// media-user-token aparece (qualquer log/depuração de config).
    #[test]
    fn sidecar_config_debug_redacts_the_session() {
        let config = SidecarConfig {
            session: Some(SidecarSession {
                media_user_token: Some("MEDIA-TOKEN-XYZ".to_owned()),
                cookies: vec![SidecarCookie {
                    name: "it.vt".to_owned(),
                    value: "COOKIE-VALUE-ABC".to_owned(),
                    domain: Some(".apple.com".to_owned()),
                }],
            }),
            ..SidecarConfig::default()
        };
        let rendered = format!("{config:?}");
        assert!(
            !rendered.contains("MEDIA-TOKEN-XYZ"),
            "debug leaks the media token: {rendered}"
        );
        assert!(
            !rendered.contains("COOKIE-VALUE-ABC"),
            "debug leaks the cookie value: {rendered}"
        );
        // Nomes de campo comuns continuam presentes para diagnósticos.
        assert!(rendered.contains("SidecarConfig"));
        assert!(rendered.contains("node_path"));
    }
}
