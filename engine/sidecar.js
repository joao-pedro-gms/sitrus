'use strict';
// sidecar.js — entrada do sidecar em Node puro. Sobe o servidor JSON-RPC sobre
// WebSocket restrito a 127.0.0.1 com porta efêmera e token de sessão gerado
// por execução (env SITRUS_SIDECAR_TOKEN, nunca logado), e despacha comandos
// para um controlador:
//   - modo "mock"       → player simulado in-process (lib/mock-player.js); usado
//                         pelos testes e CI, sem Electron nem display.
//   - modo "electron"   → bridge stdin/stdout com o main.js do Electron castlabs
//                         (lib/electron-bridge.js), que carrega music.apple.com
//                         com o CDM Widevine pinado no package.json/lock.
//   - modo "apple-mock" → idem, carregando mock/apple-mock.html (facade MusicKit
//                         para exercitar o adapter in-page sem sessão real).
//
// Sessão Apple (ponte de sessão): o cliente Rust escreve UMA linha JSON no
// stdin deste processo (envelope {v:1, mediaUserToken, cookies}) logo depois
// do spawn — o segredo sai por pipe, NUNCA por argv, env, URL ou log. O
// sidecar anuncia a linha de pronto só depois de consumir essa linha (ou do
// limite sem sessão), e no modo electron repassa por arquivo temporário 0600
// (SITRUS_SESSION_FILE), apagado pelo Electron após a leitura e também aqui
// no dispose. `session_probe` devolve o envelope/cookies observados — só
// para diagnóstico, pelo canal WS local autenticado.

const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');
const { WebSocketServer } = require('ws');
const { MockPlayer } = require('./lib/mock-player');
const { startElectronBridge } = require('./lib/electron-bridge');

const TOKEN = process.env.SITRUS_SIDECAR_TOKEN;
if (!TOKEN) {
  process.stderr.write('sitrus-engine: SITRUS_SIDECAR_TOKEN is required\n');
  process.exit(2);
}

const MODE =
  process.env.SITRUS_SIDECAR_MODE === 'mock' ? 'mock' : 'electron';
const PAGE = process.env.SITRUS_SIDECAR_PAGE === 'mock'
  ? 'mock'
  : process.env.SITRUS_SIDECAR_PAGE === 'apple-mock'
    ? 'apple-mock'
    : 'apple';
const TICK_MS = clampTick(process.env.SITRUS_SIDECAR_TICK_MS);
const AUTH_IDLE_MS = 10_000;
const MAX_PAYLOAD = 1 << 20;
const METHODS = new Set([
  'load',
  'play',
  'pause',
  'seek',
  'volume',
  'shutdown',
  'session_probe',
]);
// Tempo máximo de espera pela linha de sessão (stdin) antes de seguir sem
// sessão — clientes Rust escrevem a linha logo após o spawn.
const SESSION_ENVELOPE_TIMEOUT_MS = 300;
// Modo/limites do arquivo temporário que leva os cookies ao Electron.
const SESSION_FILE_MODE = 0o600;
const SESSION_DIR_NAME = 'sitrus-engine-sessions';
const MAX_COOKIES = 64;
const MAX_COOKIE_NAME = 256;
const MAX_COOKIE_VALUE = 4096;
const MAX_MEDIA_USER_TOKEN = 2048;

function clampTick(raw) {
  const value = Number.parseInt(raw ?? '', 10);
  if (!Number.isFinite(value) || value < 10 || value > 1000) return 100;
  return value;
}

// ————— Sessão Apple (envelope stdin) —————

let session = null; // { cookies: [{name, value, domain?}], mediaUserToken }
let sessionFilePath = null; // arquivo 0600 escrito para o Electron

// Converte a linha bruta do stdin no envelope normalizado; `null` significa
// sessão ausente/inválida (o conteúdo inválido NUNCA é impresso).
function normalizeSession(rawLine) {
  const text = String(rawLine ?? '').trim();
  if (!text) return null;
  let frame;
  try {
    frame = JSON.parse(text);
  } catch {
    return null;
  }
  if (!frame || typeof frame !== 'object' || frame.v !== 1) return null;
  const rawToken = frame.mediaUserToken;
  const mediaUserToken =
    typeof rawToken === 'string' &&
    rawToken.length > 0 &&
    rawToken.length <= MAX_MEDIA_USER_TOKEN
      ? rawToken
      : null;
  const rawCookies = Array.isArray(frame.cookies) ? frame.cookies : [];
  if (rawCookies.length > MAX_COOKIES) return null;
  const cookies = [];
  for (const item of rawCookies) {
    if (!item || typeof item !== 'object') return null;
    const name = item.name;
    const value = item.value;
    if (
      typeof name !== 'string' ||
      name.length === 0 ||
      name.length > MAX_COOKIE_NAME ||
      typeof value !== 'string' ||
      value.length === 0 ||
      value.length > MAX_COOKIE_VALUE
    ) {
      return null;
    }
    const normalized = { name, value };
    if (typeof item.domain === 'string' && item.domain) {
      normalized.domain = item.domain;
    }
    cookies.push(normalized);
  }
  if (!mediaUserToken && cookies.length === 0) return null;
  return { mediaUserToken, cookies };
}

// Escreve o envelope do Electron (arquivo 0600) e devolve o caminho.
function writeSessionFile() {
  const dir = path.join(os.tmpdir(), SESSION_DIR_NAME);
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const file = path.join(
    dir,
    `session-${process.pid}-${crypto.randomBytes(8).toString('hex')}.json`,
  );
  const payload = JSON.stringify({
    v: 1,
    mediaUserToken: session ? (session.mediaUserToken ?? null) : null,
    cookies: session ? session.cookies : [],
  });
  // mode já cria com 0600; chmod é redundante contra umask estranha.
  fs.writeFileSync(file, payload, { mode: SESSION_FILE_MODE });
  try {
    fs.chmodSync(file, SESSION_FILE_MODE);
  } catch {
    // a criação já usa mode 0600
  }
  return file;
}

function clearSessionFile() {
  if (!sessionFilePath) return;
  try {
    fs.unlinkSync(sessionFilePath);
  } catch {
    // o Electron pode já ter lido e apagado
  }
  sessionFilePath = null;
}

// ————— Backend (mock | electron) —————

function emit(params, controller) {
  // Notificação JSON-RPC para o único cliente autenticado.
  if (
    controller.activeSocket &&
    controller.activeSocket.readyState === controller.activeSocket.OPEN
  ) {
    controller.activeSocket.send(
      JSON.stringify({ jsonrpc: '2.0', method: 'event', params }),
    );
  }
}

const controller = { activeSocket: null };
let backend = null;
let backendStarted = false;
let gracefulRan = false;

function mockBackend() {
  const player = new MockPlayer({
    tickMs: TICK_MS,
    emit: (params) => emit(params, controller),
  });
  return {
    handle(method, params, reply) {
      switch (method) {
        case 'load':
          player.load(Array.isArray(params.items) ? params.items : []);
          reply(null);
          return;
        case 'play':
          player.play();
          reply(null);
          return;
        case 'pause':
          player.pause();
          reply(null);
          return;
        case 'seek':
          player.seek(Number(params.positionMs) || 0);
          reply(null);
          return;
        case 'volume':
          player.setVolume(params.volume);
          reply(null);
          return;
        case 'session_probe':
          reply(null, {
            cookies: session ? session.cookies : [],
            mediaUserToken: session && session.mediaUserToken
              ? session.mediaUserToken
              : null,
          });
          return;
        case 'shutdown':
          // Responde antes de encerrar; o fecho do socket segue logo em seguida.
          reply(null);
          setTimeout(graceful, 50).unref();
          return;
        default:
          reply({ code: -32601, message: 'unknown method' });
      }
    },
    dispose() {
      player.dispose();
    },
    mode: 'mock',
  };
}

function startBackend() {
  if (backendStarted) return;
  backendStarted = true;
  if (MODE === 'mock') {
    backend = mockBackend();
  } else {
    if (session) {
      try {
        sessionFilePath = writeSessionFile();
      } catch (error) {
        process.stderr.write(
          `sitrus-engine: could not write the session file: ${error.message}\n`,
        );
      }
    }
    backend = startElectronBridge({
      engineDir: __dirname,
      emit: (params) => emit(params, controller),
      sessionFile: sessionFilePath,
    });
  }
  onBackendReady();
}

// ————— Servidor JSON-RPC —————

const wss = new WebSocketServer({
  host: '127.0.0.1',
  port: 0,
  maxPayload: MAX_PAYLOAD,
});

let listeningPort = null;
let readyAnnounced = false;

wss.on('connection', (socket, request) => {
  // Navegadores sempre enviam Origin; os clientes legítimos (Rust/Node) nunca.
  if (request.headers.origin) {
    socket.close(1008, 'SITRUS_SIDECAR_ORIGIN_REFUSED');
    return;
  }
  if (!backendStarted) {
    socket.close(1013, 'SITRUS_SIDECAR_STARTING');
    return;
  }
  if (
    controller.activeSocket &&
    controller.activeSocket.readyState === controller.activeSocket.OPEN
  ) {
    socket.close(1013, 'SITRUS_SIDECAR_BUSY');
    return;
  }

  socket.authenticated = false;
  socket.idleTimer = setTimeout(() => {
    socket.close(1008, 'SITRUS_SIDECAR_IDLE');
  }, AUTH_IDLE_MS);

  socket.on('message', (data) => {
    clearTimeoutIfAuthenticated(socket);
    let message;
    try {
      message = JSON.parse(data.toString());
    } catch {
      socket.close(1007, 'SITRUS_SIDECAR_BAD_FRAME');
      return;
    }
    handleMessage(socket, message);
  });

  socket.on('close', () => {
    clearTimeout(socket.idleTimer);
    if (controller.activeSocket === socket) controller.activeSocket = null;
  });
  socket.on('error', () => socket.terminate());
  controller.activeSocket = socket;
});

wss.on('listening', () => {
  const address = wss.address ? wss.address() : wss._server.address();
  listeningPort = address.port;
  onBackendReady();
});

// Linha de pronto saí em stdout APENAS quando (a) o backend subiu e
// (b) a porta está anunciável — um cliente conectado não chega antes de a
// sessão (stdin) ter sido consumida.
function onBackendReady() {
  if (backendStarted && listeningPort != null && !readyAnnounced) {
    process.stdout.write(
      `SITRUS_SIDECAR_READY ${JSON.stringify({ port: listeningPort })}\n`,
    );
    readyAnnounced = true;
  }
}

function clearTimeoutIfAuthenticated(socket) {
  if (!socket.authenticated && socket.idleTimer) {
    clearTimeout(socket.idleTimer);
  }
}

function respond(socket, id, result) {
  if (id == null) return;
  socket.send(JSON.stringify({ jsonrpc: '2.0', id, result }));
}

function respondError(socket, id, error) {
  if (id == null) return;
  socket.send(JSON.stringify({ jsonrpc: '2.0', id, error }));
}

function handleSessionProbe(socket, id) {
  if (backend && backend.mode === 'electron') {
    // O dado observado está no main.js do Electron (cookies.get do perfil
    // injetado antes do loadURL); o bridge repassa a resposta.
    backend.handle('session_probe', {}, (error, result) => {
      if (error && socket.readyState === socket.OPEN) {
        respondError(socket, id, error);
      } else {
        respond(socket, id, result ?? null);
      }
    });
    return;
  }
  respond(socket, id, {
    cookies: session ? session.cookies : [],
    mediaUserToken:
      session && session.mediaUserToken ? session.mediaUserToken : null,
  });
}

function handleMessage(socket, message) {
  if (!socket.authenticated) {
    const valid =
      message &&
      message.method === 'initialize' &&
      message.params &&
      message.params.protocol === 1 &&
      message.params.token === TOKEN;
    if (!valid) {
      respondError(
        socket,
        message && message.id != null ? message.id : null,
        { code: -32001, message: 'invalid session token' },
      );
      socket.close(1008, 'SITRUS_SIDECAR_AUTH_FAILED');
      return;
    }
    socket.authenticated = true;
    clearTimeout(socket.idleTimer);
    respond(socket, message.id, { protocol: 1, mode: backend.mode, page: PAGE });
    return;
  }

  const { method, params = {}, id } = message;
  if (typeof method !== 'string' || !METHODS.has(method)) {
    respondError(socket, id, {
      code: -32601,
      message: `unknown method: ${String(method)}`,
    });
    return;
  }
  if (method === 'session_probe') {
    handleSessionProbe(socket, id);
    return;
  }
  // Falhas de reprodução vão como eventos; o RPC responde o resultado da
  // aceitação do comando (paridade com o Rust MockEngine).
  try {
    backend.handle(method, params, (error, result) => {
      if (error && socket.readyState === socket.OPEN) {
        respondError(socket, id, error);
      } else {
        respond(socket, id, result ?? null);
      }
    });
  } catch (error) {
    respondError(socket, id, {
      code: -32603,
      message: `sidecar internal error: ${error.message}`,
    });
  }
}

function graceful() {
  if (gracefulRan) {
    // stdin EOF e SIGTERM podem chegar juntos; encerra uma única vez.
    return;
  }
  gracefulRan = true;
  let done = false;
  const finish = () => {
    if (!done) {
      done = true;
      process.exit(0);
    }
  };
  let disposed = null;
  try {
    disposed = backend ? backend.dispose() : null;
  } catch {
    disposed = null;
  }
  const backendExit = disposed instanceof Promise ? disposed : Promise.resolve();
  clearSessionFile();
  for (const socket of wss.clients) {
    socket.close(1001, 'SITRUS_SIDECAR_SHUTDOWN');
  }
  wss.close();
  // O node SÓ sai quando o backend morreu de verdade (no modo electron, a
  // promessa resolve na morte real do Electron — EOF por stdio + SIGKILL em
  // 400 ms no bridge). O teto de 3 s limita a espera em qualquer caso.
  backendExit.then(finish, finish);
  setTimeout(finish, 3000).unref();
}

// ————— Sessão + ciclo de vida por stdin —————

const rl = readline.createInterface({ input: process.stdin, terminal: false });
let sessionLineSeen = false;

rl.on('line', (line) => {
  if (sessionLineSeen) return;
  sessionLineSeen = true;
  clearTimeout(sessionFallback);
  const normalized = normalizeSession(line);
  if (!normalized && line && line.trim()) {
    process.stderr.write(
      'sitrus-engine: session envelope ignored (invalid or outside the v1 contract)\n',
    );
  }
  session = normalized;
  startBackend();
});

const sessionFallback = setTimeout(() => {
  if (!sessionLineSeen) {
    sessionLineSeen = true;
    startBackend();
  }
}, SESSION_ENVELOPE_TIMEOUT_MS);

rl.on('close', graceful);
process.on('SIGTERM', graceful);
process.on('SIGINT', graceful);
