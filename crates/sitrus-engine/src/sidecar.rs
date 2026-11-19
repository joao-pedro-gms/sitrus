//! Cliente RPC do sidecar: spawn do Node, handshake autenticado e fachada
//! **não bloqueante** da trait [`crate::Engine`].
//!
//! Transporte da sessão Apple: quando a configuração carrega
//! `SidecarConfig::session`, o cliente escreve **uma linha JSON** no stdin do
//! sidecar logo após o spawn (envelope `{v, mediaUserToken, cookies}`). O
//! segredo sai por pipe — nunca por argv, env, URL ou log — e o sidecar o
//! repassa ao Electron por arquivo temporário 0600 apagado após a leitura.
//!
//! Modelo de concorrência: os comandos da trait são **enfileirados numa
//! thread de fundo dedicada (actor)** e o chamador NUNCA espera a resposta —
//! o `Result` é da validação local/infraestrutura (fila, volume, canal
//! fechado). Os resultados (estado, posição, falhas) chegam por
//! [`EngineEvent`]; falhas de comandos enfileirados chegam como
//! [`EngineEvent::CommandFailed`], com o prazo de cada RPC imposto pelo
//! actor (nada de `recv_timeout` no thread que chamou o comando).
//! [`spawn`] continua síncrono/bloqueante por natureza (subir um processo e
//! fazer handshake) e deve ser chamado de uma thread de fundo, não da UI.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tracing::warn;

use crate::engine::{Engine, EngineEvent, EngineState, QueueItem, validate_volume};
use crate::error::EngineError;
use crate::rpc::{
    self, IncomingMessage, MethodCall, RpcRequestFrame, SessionProbe, SidecarConfig, SidecarMode,
    SidecarPage, WireQueueItem,
};

/// Execução ativa do sidecar.
#[derive(Debug)]
pub struct SidecarEngine {
    event_receiver: Receiver<EngineEvent>,
    state: Arc<Mutex<EngineState>>,
    commands: tokio_mpsc::UnboundedSender<ActorRequest>,
    child: Arc<Mutex<Option<Child>>>,
    stdin: Mutex<Option<ChildStdin>>,
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug)]
struct ActorRequest {
    call: MethodCall,
    responder: Responder,
}

/// Resolvedor de um RPC pendente: resultado opcional (`session_probe`) ou erro.
type Responder = mpsc::Sender<Result<Option<serde_json::Value>, EngineError>>;

#[derive(Debug)]
struct ActorContext {
    state: Arc<Mutex<EngineState>>,
    events_tx: mpsc::Sender<EngineEvent>,
    /// RPCs aguardando resposta, por `id`, com o prazo (imposto aqui, no
    /// actor — o chamador nunca espera).
    pending: HashMap<u64, (Responder, Instant)>,
    commands: tokio_mpsc::UnboundedReceiver<ActorRequest>,
    handshake_timeout: Duration,
    rpc_timeout: Duration,
}

impl SidecarEngine {
    /// Sobe o sidecar (`node <engine>/sidecar.js`), autentica o WebSocket e
    /// devolve a fachada. Falhas de disponibilidade são tipadas: Node
    /// ausente, script faltando, linha de pronto fora do prazo, handshake
    /// recusado — todas pedem fallback nativo (ver
    /// [`EngineError::requires_native_fallback`]).
    pub fn spawn(config: &SidecarConfig) -> Result<Self, EngineError> {
        let entry = config.entry_script();
        if !entry.is_file() {
            return Err(EngineError::SidecarUnavailable {
                reason: format!("sidecar entry script is missing: {}", entry.display()),
            });
        }
        // A sessão é validada ANTES de qualquer processo existir: entrada
        // local inválida é rejeitada sem sidecar.
        if let Some(session) = &config.session {
            session.validate()?;
        }
        let token = generate_token()?;
        let mut command = Command::new(&config.node_path);
        command
            .arg(&entry)
            .current_dir(&config.engine_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.envs(sidecar_envs(config, &token));

        let mut child = command
            .spawn()
            .map_err(|error| EngineError::SidecarUnavailable {
                reason: format!("could not spawn node sidecar: {error}"),
            })?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Transporte da sessão (Rust → sidecar): uma linha JSON no stdin,
        // escrita ANTES da espera da linha de pronto — o sidecar só anuncia a
        // porta depois de resolver a sessão. O segredo sai por pipe, nunca
        // por argv/env/URL, e o descritor é exclusivo do processo filho.
        if let Some(session) = &config.session {
            let frame = session.envelope();
            let delivered = stdin
                .as_mut()
                .map(|file| file.write_all(frame.as_bytes()).and_then(|_| file.flush()))
                .unwrap_or(Err(std::io::Error::other("child stdin unavailable")));
            if let Err(error) = delivered {
                let _ = child.kill();
                drop(stdin.take());
                return Err(EngineError::SidecarUnavailable {
                    reason: format!("could not hand the session to the sidecar stdin: {error}"),
                });
            }
        }

        // stdout: só a linha de pronto; stderr: cauda para diagnósticos.
        let (ready_tx, ready_rx) = mpsc::channel::<String>();
        if let Some(stdout) = stdout {
            thread::Builder::new()
                .name("sitrus-sidecar-stdout".to_owned())
                .spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if ready_tx.send(line).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| EngineError::SidecarUnavailable {
                    reason: format!("could not watch sidecar stdout: {error}"),
                })?;
        }
        let stderr_tail = Arc::new(Mutex::new(Vec::<String>::new()));
        if let Some(stderr) = stderr {
            let tail = Arc::clone(&stderr_tail);
            thread::Builder::new()
                .name("sitrus-sidecar-stderr".to_owned())
                .spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        // Mutex envenenado: descarta a linha, sem panic (m4).
                        let _ = tail.lock().map(|mut guard| {
                            guard.push(line);
                            if guard.len() > 8 {
                                guard.remove(0);
                            }
                        });
                    }
                })
                .map_err(|error| EngineError::SidecarUnavailable {
                    reason: format!("could not watch sidecar stderr: {error}"),
                })?;
        }

        let port = wait_ready_line(&ready_rx, config.spawn_timeout, &mut child, &stderr_tail)?;

        // Runtime dedicado: connect WS + handshake + loop actor.
        let (event_sender, event_receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(EngineState::Stopped));
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();
        let (setup_tx, setup_rx) = mpsc::channel::<Result<(), EngineError>>();

        let ctx = ActorContext {
            state: Arc::clone(&state),
            events_tx: event_sender,
            pending: HashMap::new(),
            commands: command_receiver,
            handshake_timeout: config.handshake_timeout,
            rpc_timeout: config.rpc_timeout,
        };
        thread::Builder::new()
            .name("sitrus-engine".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(actor_main(port, token, ctx, setup_tx)),
                    Err(error) => {
                        let _ = setup_tx.send(Err(EngineError::SidecarUnavailable {
                            reason: format!("could not start async runtime: {error}"),
                        }));
                    }
                }
            })
            .map_err(|error| EngineError::SidecarUnavailable {
                reason: format!("could not start engine thread: {error}"),
            })?;

        let guard = config.spawn_timeout + config.handshake_timeout + Duration::from_secs(2);
        match setup_rx.recv_timeout(guard) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = child.kill();
                drop(stdin.take());
                return Err(error);
            }
            Err(_) => {
                let _ = child.kill();
                drop(stdin.take());
                return Err(EngineError::SidecarTimedOut {
                    timeout_ms: u64::try_from(guard.as_millis()).unwrap_or(u64::MAX),
                });
            }
        }

        Ok(Self {
            event_receiver,
            state,
            commands: command_sender,
            child: Arc::new(Mutex::new(Some(child))),
            stdin: Mutex::new(stdin),
            stderr_tail: Arc::clone(&stderr_tail),
        })
    }

    /// Enfileira um comando RPC sem bloquear o chamador.
    ///
    /// O `Result` é da infraestrutura local: canal fechado = engine caiu
    /// ([`EngineError::ConnectionLost`]). A resposta (ou o erro) do RPC chega
    /// por [`EngineEvent::CommandFailed`] no canal de eventos, com o prazo
    /// imposto pelo actor — nada de espera no thread que enfileirou.
    fn enqueue_command(&self, call: MethodCall) -> Result<(), EngineError> {
        let (responder, _response) = mpsc::channel();
        self.commands
            .send(ActorRequest { call, responder })
            .map_err(|_| EngineError::ConnectionLost)
    }

    /// Enfileira um comando e ESPERA a resposta; uso exclusivo de
    /// diagnóstico ([`session_probe`], nunca do thread de UI em produção).
    /// O prazo é imposto pelo actor: bloqueia no máximo `rpc_timeout` + latência.
    fn request_result_blocking(
        &self,
        call: MethodCall,
    ) -> Result<Option<serde_json::Value>, EngineError> {
        let (responder, response) = mpsc::channel();
        self.commands
            .send(ActorRequest { call, responder })
            .map_err(|_| EngineError::ConnectionLost)?;
        match response.recv() {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(EngineError::ConnectionLost),
        }
    }

    /// Diagnóstico da ponte de sessão: devolve o que o LADO DO SIDECAR
    /// observou. No modo mock é o echo do envelope recebido por stdin; no
    /// modo electron é a saída de `session.defaultSession.cookies.get` do
    /// perfil do Electron (cookies injetados antes do `loadURL`) com o
    /// media-user-token recebido. Os valores trafegam apenas neste canal
    /// local autenticado — nenhum log os imprime.
    ///
    /// Bloqueante por natureza (diagnóstico/testes): NÃO use no thread de UI.
    ///
    /// # Erros
    /// [`EngineError::Rpc`]/[`EngineError::RpcTimedOut`] quando o sidecar não
    /// responde; [`EngineError::InvalidMessage`] quando o payload não casa
    /// com [`SessionProbe`].
    pub fn session_probe(&self) -> Result<SessionProbe, EngineError> {
        let raw = self.request_result_blocking(MethodCall::SessionProbe {})?;
        let Some(raw) = raw else {
            return Err(EngineError::InvalidMessage {
                reason: "session_probe returned no payload".to_owned(),
            });
        };
        serde_json::from_value(raw).map_err(|error| EngineError::InvalidMessage {
            reason: format!("session_probe payload is not a session probe: {error}"),
        })
    }

    /// PID do processo Node do sidecar, quando vivo. Usado por verificações
    /// de superfície de processo (por exemplo, `/proc/<pid>/{cmdline,environ}`).
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.child
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|child| child.id()))
    }

    /// Cauda recente do stderr do sidecar (até 8 linhas, diagnósticos). Por
    /// contrato não contém segredos; os testes usam para provar isso.
    #[must_use]
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|tail| tail.clone())
            .unwrap_or_default()
    }
}

impl Engine for SidecarEngine {
    fn load_queue(&self, queue: Vec<QueueItem>) -> Result<(), EngineError> {
        for item in &queue {
            item.validate()?;
        }
        self.enqueue_command(MethodCall::Load {
            items: queue.iter().map(WireQueueItem::from).collect(),
        })
    }

    fn play(&self) -> Result<(), EngineError> {
        self.enqueue_command(MethodCall::Play {})
    }

    fn pause(&self) -> Result<(), EngineError> {
        self.enqueue_command(MethodCall::Pause {})
    }

    fn seek(&self, position: Duration) -> Result<(), EngineError> {
        self.enqueue_command(MethodCall::Seek {
            position_ms: u64::try_from(position.as_millis()).unwrap_or(u64::MAX),
        })
    }

    fn set_volume(&self, volume: f32) -> Result<(), EngineError> {
        validate_volume(volume)?;
        self.enqueue_command(MethodCall::Volume { volume })
    }

    fn state(&self) -> EngineState {
        self.state
            .lock()
            .map(|guard| *guard)
            .unwrap_or(EngineState::Stopped)
    }

    fn events(&self) -> &Receiver<EngineEvent> {
        &self.event_receiver
    }
}

impl Drop for SidecarEngine {
    fn drop(&mut self) {
        // Fecha stdin: o sidecar encerra o filho Electron e o servidor WS.
        // O reap do processo NÃO bloqueia o thread que derruba o engine
        // (a UI): um reaper detached aguarda a carência curta e faz o kill
        // síncrono (parte do contrato anti-órfãos, cf. fix-session).
        if let Ok(mut guard) = self.stdin.lock() {
            drop(guard.take());
        }
        if let Ok(mut slot) = self.child.lock()
            && let Some(mut child) = slot.take()
        {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::Builder::new()
                .name("sitrus-sidecar-reaper".to_owned())
                .spawn(move || {
                    for _ in 0..12 {
                        match child.try_wait() {
                            Ok(Some(_)) | Err(_) => return,
                            Ok(None) => thread::sleep(Duration::from_millis(50)),
                        }
                    }
                    let _ = child.kill();
                })
                .ok();
        }
    }
}

/// Variáveis de ambiente do lado do Node. O token só vive no processo do
/// sidecar (o bridge Electron o remove passando adiante) e nunca é logado.
fn sidecar_envs(config: &SidecarConfig, token: &str) -> HashMap<String, String> {
    let mut envs = HashMap::new();
    envs.insert("SITRUS_SIDECAR_TOKEN".to_owned(), token.to_owned());
    envs.insert(
        "SITRUS_SIDECAR_MODE".to_owned(),
        match config.mode {
            SidecarMode::Electron => "electron",
            SidecarMode::Mock => "mock",
        }
        .to_owned(),
    );
    envs.insert(
        "SITRUS_SIDECAR_PAGE".to_owned(),
        match config.page {
            SidecarPage::AppleMusic => "apple",
            SidecarPage::AppleMock => "apple-mock",
            SidecarPage::Mock => "mock",
        }
        .to_owned(),
    );
    envs.insert(
        "SITRUS_SIDECAR_SHOW".to_owned(),
        if config.show_window { "1" } else { "0" }.to_owned(),
    );
    if let Some(tick_ms) = config.tick_ms {
        envs.insert("SITRUS_SIDECAR_TICK_MS".to_owned(), tick_ms.to_string());
    }
    if let Some(user_data) = &config.user_data_dir {
        envs.insert(
            "SITRUS_SIDECAR_USER_DATA".to_owned(),
            user_data.display().to_string(),
        );
    }
    if !config.electron_args.is_empty() {
        envs.insert(
            "SITRUS_SIDECAR_ELECTRON_ARGS_JSON".to_owned(),
            serde_json::to_string(&config.electron_args).unwrap_or_else(|_| "[]".to_owned()),
        );
    }
    envs
}

/// Espera `SITRUS_SIDECAR_READY {"port": N}` respeitando o prazo e o estado
/// do processo; linhas estranhas no stdout são ignoradas (e logadas).
fn wait_ready_line(
    ready_rx: &Receiver<String>,
    deadline: Duration,
    child: &mut Child,
    stderr_tail: &Arc<Mutex<Vec<String>>>,
) -> Result<u16, EngineError> {
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() >= deadline {
            return Err(EngineError::SidecarTimedOut {
                timeout_ms: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
            });
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        match ready_rx.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(line) => match rpc::parse_ready_line(&line) {
                Ok(ready) => return Ok(ready.port),
                Err(_) => {
                    warn!(target: "sitrus_engine", "sidecar stdout stray line (ignored)");
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    let tail = stderr_tail
                        .lock()
                        .map(|tail| tail.join("\n"))
                        .unwrap_or_default();
                    return Err(EngineError::SidecarUnavailable {
                        reason: format!(
                            "sidecar exited before readiness (status: {status}); stderr tail: {tail}"
                        ),
                    });
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(EngineError::SidecarUnavailable {
                    reason: "sidecar stdout closed unexpectedly".to_owned(),
                });
            }
        }
    }
}

/// Gera o token de sessão por execução (32 bytes de `/dev/urandom` em hex).
fn generate_token() -> Result<String, EngineError> {
    use std::io::Read as _;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| EngineError::SidecarUnavailable {
            reason: format!("could not generate session token: {error}"),
        })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Conecta no WebSocket, autentica e roda o loop actor até a conexão cair.
async fn actor_main(
    port: u16,
    token: String,
    mut ctx: ActorContext,
    setup_tx: mpsc::Sender<Result<(), EngineError>>,
) {
    let uri = format!("ws://127.0.0.1:{port}/");
    let connect = tokio::time::timeout(ctx.handshake_timeout, connect_async(&uri)).await;
    let (mut stream, _) = match connect {
        Ok(Ok(pair)) => pair,
        Ok(Err(error)) => {
            let _ = setup_tx.send(Err(EngineError::SidecarUnavailable {
                reason: format!("could not connect to sidecar websocket: {error}"),
            }));
            return;
        }
        Err(_) => {
            let _ = setup_tx.send(Err(EngineError::HandshakeRejected {
                reason: format!(
                    "websocket connect timed out after {} ms",
                    ctx.handshake_timeout.as_millis()
                ),
            }));
            return;
        }
    };

    let handshake = run_handshake(&mut stream, &token, ctx.handshake_timeout).await;
    if let Err(error) = handshake {
        let _ = setup_tx.send(Err(error));
        return;
    }
    if setup_tx.send(Ok(())).is_err() {
        return;
    }

    let (mut write, mut read) = stream.split();
    let mut next_id: u64 = 1;
    loop {
        // Prazo mais próximo entre os RPCs pendentes: o select acorda nele,
        // resolvendo o timeout NO LADO DO ACTOR.
        let next_deadline = ctx.pending.values().map(|(_, deadline)| *deadline).min();
        let timer = async {
            match next_deadline {
                Some(deadline) => {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                }
                None => {
                    std::future::pending::<()>().await;
                }
            }
        };
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(Message::Ping(payload))) => {
                        if write.send(Message::Pong(payload)).await.is_err() {
                            finish_disconnected(&mut ctx, "sidecar write failed on pong");
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        handle_frame(&mut ctx, text.as_bytes());
                    }
                    Some(Ok(Message::Binary(data))) => {
                        handle_frame(&mut ctx, &data);
                    }
                    Some(Ok(Message::Close(_))) => {
                        finish_disconnected(&mut ctx, "sidecar closed the websocket");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        finish_disconnected(&mut ctx, &error.to_string());
                        break;
                    }
                    None => {
                        finish_disconnected(&mut ctx, "sidecar stream ended");
                        break;
                    }
                }
            }
            request = ctx.commands.recv() => {
                match request {
                    Some(ActorRequest { call, responder }) => {
                        let id = next_id;
                        next_id += 1;
                        match serde_json::to_string(&RpcRequestFrame {
                            jsonrpc: "2.0",
                            id,
                            call: &call,
                        }) {
                            Ok(frame) => {
                                ctx.pending.insert(
                                    id,
                                    (responder, Instant::now() + ctx.rpc_timeout),
                                );
                                if write.send(Message::Text(frame.into())).await.is_err() {
                                    finish_disconnected(&mut ctx, "sidecar write failed");
                                    break;
                                }
                            }
                            Err(error) => {
                                let _ = responder.send(Err(EngineError::InvalidMessage {
                                    reason: format!("could not serialize request: {error}"),
                                }));
                            }
                        }
                    }
                    None => {
                        // Engine descartado: fecha com elegância.
                        let _ = write.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            _ = timer => {
                resolve_expired(&mut ctx);
            }
        }
    }
    finish_disconnected(&mut ctx, "sidecar actor loop ended");
}

/// Resolve os RPCs pendentes que estouraram o prazo: erro para o chamador
/// bloqueante (diagnóstico); sem chamador esperando, o erro tipado vira
/// [`EngineEvent::CommandFailed`] — visível onde o comando foi enfileirado.
fn resolve_expired(ctx: &mut ActorContext) {
    let now = Instant::now();
    let expired: Vec<u64> = ctx
        .pending
        .iter()
        .filter(|(_, (_, deadline))| *deadline <= now)
        .map(|(id, _)| *id)
        .collect();
    for id in expired {
        let Some((responder, _)) = ctx.pending.remove(&id) else {
            continue;
        };
        let error = EngineError::RpcTimedOut {
            timeout_ms: u64::try_from(ctx.rpc_timeout.as_millis()).unwrap_or(u64::MAX),
        };
        if let Err(mpsc::SendError(Err(timed_out))) = responder.send(Err(error)) {
            let _ = ctx.events_tx.send(EngineEvent::CommandFailed(timed_out));
        }
    }
}

async fn run_handshake(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    token: &str,
    timeout: Duration,
) -> Result<(), EngineError> {
    let request = serde_json::to_string(&RpcRequestFrame {
        jsonrpc: "2.0",
        id: 0,
        call: &MethodCall::Initialize {
            protocol: rpc::PROTOCOL_VERSION,
            token: token.to_owned(),
        },
    })
    .map_err(|error| EngineError::InvalidMessage {
        reason: format!("could not serialize initialize: {error}"),
    })?;
    let send = tokio::time::timeout(timeout, stream.send(Message::Text(request.into()))).await;
    match send {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(EngineError::HandshakeRejected {
                reason: format!("websocket send failed: {error}"),
            });
        }
        Err(_) => {
            return Err(EngineError::HandshakeRejected {
                reason: format!("initialize send timed out after {} ms", timeout.as_millis()),
            });
        }
    }

    let response = tokio::time::timeout(timeout, stream.next()).await;
    match response {
        Ok(Some(Ok(Message::Text(text)))) => resolve_handshake_response(text.as_bytes()),
        Ok(Some(Ok(Message::Binary(data)))) => resolve_handshake_response(&data),
        Ok(Some(Ok(Message::Close(frame)))) => Err(EngineError::HandshakeRejected {
            reason: format!(
                "sidecar closed the connection during handshake ({})",
                frame.as_ref().map_or_else(
                    || "no close frame".to_owned(),
                    |frame| frame.reason.as_str().to_owned(),
                )
            ),
        }),
        Ok(Some(Ok(_))) => Err(EngineError::InvalidMessage {
            reason: "handshake expects a text frame".to_owned(),
        }),
        Ok(Some(Err(error))) => Err(EngineError::HandshakeRejected {
            reason: format!("handshake failed: {error}"),
        }),
        Ok(None) => Err(EngineError::HandshakeRejected {
            reason: "sidecar closed the connection during handshake".to_owned(),
        }),
        Err(_) => Err(EngineError::HandshakeRejected {
            reason: format!("handshake timed out after {} ms", timeout.as_millis()),
        }),
    }
}

fn resolve_handshake_response(text: &[u8]) -> Result<(), EngineError> {
    let value: serde_json::Value =
        serde_json::from_slice(text).map_err(|error| EngineError::InvalidMessage {
            reason: format!("handshake frame is not JSON: {error}"),
        })?;
    match IncomingMessage::parse(value)? {
        IncomingMessage::Response(frame) => match frame.error {
            Some(error) => Err(EngineError::from_rpc_error(error.code, &error.message)),
            None => Ok(()),
        },
        IncomingMessage::Event(_) => Err(EngineError::InvalidMessage {
            reason: "handshake expects a response, got an event".to_owned(),
        }),
    }
}

fn handle_frame(ctx: &mut ActorContext, text: &[u8]) {
    let value: serde_json::Value = match serde_json::from_slice(text) {
        Ok(value) => value,
        Err(error) => {
            warn!(target: "sitrus_engine", "sidecar frame is not JSON: {error}");
            return;
        }
    };
    let message = match IncomingMessage::parse(value) {
        Ok(message) => message,
        Err(error) => {
            warn!(target: "sitrus_engine", "sidecar frame rejected: {error}");
            return;
        }
    };
    match message {
        IncomingMessage::Response(frame) => {
            let outcome = frame.error.as_ref().map_or_else(
                || Ok(frame.result.clone()),
                |error| Err(EngineError::from_rpc_error(error.code, &error.message)),
            );
            let Some((responder, _)) = ctx.pending.remove(&frame.id) else {
                return;
            };
            // Chamador bloqueante (diagnóstico) recebe o resultado direto;
            // comandos enfileirados (sem chamador) entregam o erro tipado
            // pela fila de eventos — resultado Ok não precisa de evento.
            if let Err(mpsc::SendError(Err(error))) = responder.send(outcome) {
                let _ = ctx.events_tx.send(EngineEvent::CommandFailed(error));
            }
        }
        IncomingMessage::Event(event) => {
            if let Ok(event) = EngineEvent::try_from(event) {
                if let EngineEvent::StateChanged(state) = event
                    && let Ok(mut guard) = ctx.state.lock()
                {
                    *guard = state;
                }
                let _ = ctx.events_tx.send(event);
            }
        }
    }
}

fn finish_disconnected(ctx: &mut ActorContext, reason: &str) {
    warn!(target: "sitrus_engine", "sidecar connection lost: {reason}");
    let _ = ctx
        .events_tx
        .send(EngineEvent::CommandFailed(EngineError::ConnectionLost));
    drain_pending(ctx);
}

fn drain_pending(ctx: &mut ActorContext) {
    for (_, (responder, _)) in ctx.pending.drain() {
        // Sem chamador esperando: o CommandFailed(ConnectionLost) já foi
        // publicado por `finish_disconnected` — nada duplicado aqui.
        let _ = responder.send(Err(EngineError::ConnectionLost));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{SidecarCookie, SidecarSession};

    /// O env do Node e o argv do sidecar nunca carregam nem um byte da sessão
    /// (o valor do cookie e o media-user-token viajam só pelo stdin).
    #[test]
    fn sidecar_env_and_entry_argv_never_carry_the_session_secrets() {
        let cookie_value = "SITRUS-TESTER-COOKIE-9f3a2b";
        let media_token = "SITRUS-TESTER-MEDIA-TOKEN-77c1";
        let config = SidecarConfig {
            node_path: "/usr/bin/node".to_owned(),
            engine_dir: "/opt/sitrus/engine".into(),
            mode: SidecarMode::Electron,
            page: SidecarPage::AppleMusic,
            session: Some(SidecarSession {
                media_user_token: Some(media_token.to_owned()),
                cookies: vec![SidecarCookie {
                    name: "it.vt".to_owned(),
                    value: cookie_value.to_owned(),
                    domain: None,
                }],
            }),
            ..SidecarConfig::default()
        };
        let token = "SITRUS-TESTER-SIDECAR-TOKEN-41e5";
        let envs = sidecar_envs(&config, token);
        let joined = envs
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !joined.contains(cookie_value),
            "cookie value must not reach the sidecar env: {joined}"
        );
        assert!(
            !joined.contains(media_token),
            "media user token must not reach the sidecar env: {joined}"
        );
        // O argv visível no /proc é `node <entry>` — nenhum valor da sessão.
        let argv = format!("{} {}", config.node_path, config.entry_script().display());
        assert!(
            !argv.contains(cookie_value),
            "argv leaks the cookie: {argv}"
        );
        assert!(
            !argv.contains(media_token),
            "argv leaks the media token: {argv}"
        );
    }

    /// O envelope é UMA linha JSON exacta: versão, token opcional e cookies
    /// com nome/valor/dominio (dominio ausente é omitido).
    #[test]
    fn session_envelope_serializes_as_a_single_json_line() {
        let session = SidecarSession {
            media_user_token: Some("media-token".to_owned()),
            cookies: vec![SidecarCookie {
                name: "it.vt".to_owned(),
                value: "cookie-value".to_owned(),
                domain: Some(".apple.com".to_owned()),
            }],
        };
        assert_eq!(
            session.envelope(),
            "{\"v\":1,\"mediaUserToken\":\"media-token\",\"cookies\":[{\"name\":\"it.vt\",\"value\":\"cookie-value\",\"domain\":\".apple.com\"}]}\n"
        );
        let tokenless = SidecarSession {
            media_user_token: None,
            cookies: vec![SidecarCookie {
                name: "geo".to_owned(),
                value: "br".to_owned(),
                domain: None,
            }],
        };
        assert_eq!(
            tokenless.envelope(),
            "{\"v\":1,\"cookies\":[{\"name\":\"geo\",\"value\":\"br\"}]}\n"
        );
    }

    /// Validação rejeita envelopes vazios e cookies malformados com entrada
    /// local inválida (sem tocar em qualquer processo).
    #[test]
    fn session_validation_rejects_empty_or_malformed_envelopes() {
        let empty = SidecarSession::default();
        assert!(matches!(
            empty.validate(),
            Err(EngineError::SessionInvalid { .. })
        ));

        let nameless = SidecarSession {
            media_user_token: Some("token".to_owned()),
            cookies: vec![SidecarCookie {
                name: "   ".to_owned(),
                value: "v".to_owned(),
                domain: None,
            }],
        };
        assert!(matches!(
            nameless.validate(),
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

        let token_only = SidecarSession {
            media_user_token: Some("media-token".to_owned()),
            cookies: Vec::new(),
        };
        assert!(token_only.validate().is_ok());
    }

    /// Fachada construída em teste: mesma estrutura do spawn, sem processo.
    fn test_facade(
        commands: tokio_mpsc::UnboundedSender<ActorRequest>,
        event_receiver: Receiver<EngineEvent>,
        state: Arc<Mutex<EngineState>>,
    ) -> SidecarEngine {
        SidecarEngine {
            event_receiver,
            state,
            commands,
            child: Arc::new(Mutex::new(None)),
            stdin: Mutex::new(None),
            stderr_tail: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// COMANDO LENTO NÃO BLOQUEIA O CAMINHO DA UI: com um "sidecar" que só
    /// responde o handshake e depois fica mudo, cada comando da trait volta
    /// em milissegundos (enfileirado no actor); o prazo estoura NO ACTOR e o
    /// erro chega como [`EngineEvent::CommandFailed`] — nada de
    /// `blocking_send`/`recv_timeout` no thread que chama `play()`.
    #[test]
    fn slow_engine_never_blocks_the_caller_and_times_out_at_the_actor() {
        // A porta do falso sidecar sai de dentro do runtime dele (o listener
        // do tokio tem que viver no mesmo I/O driver do accept).
        let (port_tx, port_rx) = mpsc::channel();

        // Falso sidecar: WS de verdade (o mesmo crate), handshake Ok e SILENCIO.
        let fake = thread::Builder::new()
            .name("test-fake-sidecar".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("test runtime");
                runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                        .await
                        .expect("listener bind");
                    if port_tx
                        .send(listener.local_addr().expect("local addr").port())
                        .is_err()
                    {
                        return;
                    }
                    let (stream, _) = listener.accept().await.expect("accept tcp");
                    let mut stream = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("ws handshake");
                    let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
                        .await
                        .expect("initialize frame within 5s");
                    match first {
                        Some(Ok(Message::Text(text))) => {
                            assert!(
                                text.contains(r#""method":"initialize""#),
                                "first frame must be initialize: {text}"
                            );
                        }
                        unexpected => panic!("unexpected first frame: {unexpected:?}"),
                    }
                    let reply = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 0,
                        "result": null,
                    });
                    stream
                        .send(Message::Text(reply.to_string().into()))
                        .await
                        .expect("initialize reply");
                    // Ignora os comandos (engine lento) e fecha no fim da janela.
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                });
            })
            .expect("fake sidecar thread");

        let port = port_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fake sidecar port");
        let (event_sender, event_receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(EngineState::Stopped));
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();
        let (setup_tx, setup_rx) = mpsc::channel::<Result<(), EngineError>>();

        let ctx = ActorContext {
            state: Arc::clone(&state),
            events_tx: event_sender,
            pending: HashMap::new(),
            commands: command_receiver,
            handshake_timeout: Duration::from_secs(5),
            rpc_timeout: Duration::from_millis(300),
        };
        let actor = thread::Builder::new()
            .name("test-engine-actor".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("actor runtime");
                runtime.block_on(actor_main(
                    port,
                    "SITRUS-TEST-TOKEN".to_owned(),
                    ctx,
                    setup_tx,
                ));
            })
            .expect("actor thread");

        assert_eq!(
            setup_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("setup result"),
            Ok(()),
            "actor must authenticate against the fake sidecar"
        );

        let engine = test_facade(command_sender, event_receiver, state);

        // Enfileira 25 comandos contra um sidecar mudo: NENHUM comando bloqueia.
        let started = Instant::now();
        for _ in 0..25 {
            engine.play().expect("queue the play command");
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "queued commands must not block: {elapsed:?}"
        );
        // State lê sem bloquear (mutex local compartilhado com o actor).
        assert_eq!(engine.state(), EngineState::Stopped);

        // O prazo estourou no actor: o erro tipado chega como evento.
        let command_failed = engine
            .events()
            .recv_timeout(Duration::from_secs(2))
            .expect("timeout resolution reaches the event channel");
        assert!(
            matches!(
                command_failed,
                EngineEvent::CommandFailed(EngineError::RpcTimedOut { .. })
            ),
            "slow commands resolve as typed RpcTimedOut at the actor: {command_failed:?}"
        );

        drop(engine);
        let _ = actor.join();
        let _ = fake.join();
    }

    /// Canal do actor fechado (engine desconectado): o comando devolve
    /// `ConnectionLost` imediatamente, sem bloqueio e sem espera.
    #[test]
    fn command_into_closed_engine_channel_is_connection_lost_without_blocking() {
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();
        drop(command_receiver);
        let engine = test_facade(
            command_sender,
            mpsc::channel().1,
            Arc::new(Mutex::new(EngineState::Stopped)),
        );
        let started = Instant::now();
        assert!(matches!(engine.play(), Err(EngineError::ConnectionLost)));
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "closed channel must fail fast: {:?}",
            started.elapsed()
        );
    }
}
