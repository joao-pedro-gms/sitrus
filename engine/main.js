'use strict';
// main.js — app Electron castlabs mínimo. Espera o CDM Widevine com
// components.whenReady() (o fluxo recomendado do ECS) e abre a janela com o
// preload: por padrão music.apple.com (DRM), a página mock para provar o
// protocolo sem sessão Apple, ou a página apple-mock (facade MusicKit) para
// exercitar o adapter in-page. Comunica com o sidecar por stdin/stdout em
// JSON lines; rede alguma é aberta daqui — o WebSocket fica no sidecar.js.
//
// Ponte de sessão Apple: o sidecar escreve os cookies + media-user-token em
// SITRUS_SESSION_FILE (arquivo temporário 0600). ANTES de criar a janela e
// de qualquer loadURL, este processo lê o arquivo, APAGA, e injeta os
// cookies em session.defaultSession.cookies.set (domínios .apple.com /
// music.apple.com) — a página abre autenticada. Nada do conteúdo é logado.

const { app, components, BrowserWindow, ipcMain, session } = require('electron');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

const PAGE_ENV =
  process.env.SITRUS_SIDECAR_PAGE === 'mock'
    ? 'mock'
    : process.env.SITRUS_SIDECAR_PAGE === 'apple-mock'
      ? 'apple-mock'
      : 'apple';
const PAGE =
  PAGE_ENV === 'apple'
    ? 'https://music.apple.com/'
    : path.join(
        __dirname,
        'mock',
        PAGE_ENV === 'mock' ? 'index.html' : 'apple-mock.html',
      );
// O adapter in-page atua em páginas de player web (apple e apple-mock); a
// página mock de protocolo mantém o controlador direto (mock/page.js).
const USE_PAGE_ADAPTER = PAGE_ENV !== 'mock';
const SHOW = process.env.SITRUS_SIDECAR_SHOW === '1';
const TICK_MS = Number(process.env.SITRUS_SIDECAR_TICK_MS || '100') || 100;
const COOKIE_TARGET_URL = 'https://music.apple.com/';
const DEFAULT_COOKIE_DOMAIN = '.apple.com';

// Nome próprio antes do ready: userData default vira ~/.config/sitrus-engine
// (o CDM Widevine instalado em runtime fica isolado lá, como no Sidra).
app.setName('sitrus-engine');

if (process.env.SITRUS_SIDECAR_USER_DATA) {
  app.setPath('userData', process.env.SITRUS_SIDECAR_USER_DATA);
}

// ————— Adapter in-page —————

function readPageAdapterSource() {
  try {
    return fs.readFileSync(path.join(__dirname, 'lib', 'page-adapter.js'), 'utf8');
  } catch {
    process.stderr.write('sitrus-engine: page adapter source missing\n');
    return null;
  }
}
const PAGE_ADAPTER_SOURCE = USE_PAGE_ADAPTER ? readPageAdapterSource() : null;

let window = null;
let rendererReady = false;
const queued = [];

// ————— Sessão Apple —————

let appleSession = null; // { mediaUserToken, cookies } — em memória apenas
let resolveSessionReady = null;
let sessionReady = new Promise((resolve) => {
  resolveSessionReady = resolve;
});
let resolveWindowReady = null;
let windowReady = new Promise((resolve) => {
  resolveWindowReady = resolve;
});

// Lê e APAGA o arquivo de sessão (lido uma única vez, no arranque).
function consumeSessionFile() {
  const file = process.env.SITRUS_SESSION_FILE;
  delete process.env.SITRUS_SESSION_FILE;
  if (!file) return null;
  let raw = null;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch (error) {
    process.stderr.write(
      `sitrus-engine: session file unreadable (${error.code ?? 'error'}), continuing without an Apple session\n`,
    );
    return null;
  } finally {
    try {
      fs.unlinkSync(file);
    } catch {
      // o dispose do sidecar/bridge também tenta
    }
  }
  try {
    const frame = JSON.parse(raw);
    if (!frame || typeof frame !== 'object' || frame.v !== 1) {
      throw new Error('malformed');
    }
    const cookies = Array.isArray(frame.cookies)
      ? frame.cookies.filter(
          (cookie) =>
            cookie &&
            typeof cookie.name === 'string' &&
            typeof cookie.value === 'string' &&
            cookie.value,
        )
      : [];
    return {
      mediaUserToken:
        typeof frame.mediaUserToken === 'string' && frame.mediaUserToken
          ? frame.mediaUserToken
          : null,
      cookies,
    };
  } catch {
    process.stderr.write(
      'sitrus-engine: session envelope malformed, continuing without an Apple session\n',
    );
    return null;
  }
}

// Injeta os cookies ANTES de qualquer navegação; falha por cookie é evento
// de erro sem o valor do cookie. mediaUserToken fica em memória (probe).
function injectAppleSession() {
  appleSession = consumeSessionFile();
  if (!appleSession || appleSession.cookies.length === 0) {
    return Promise.resolve();
  }
  let injected = 0;
  const attempts = [];
  for (const cookie of appleSession.cookies) {
    attempts.push(
      session.defaultSession.cookies
        .set({
          url: COOKIE_TARGET_URL,
          name: cookie.name,
          value: cookie.value,
          domain:
            typeof cookie.domain === 'string' && cookie.domain
              ? cookie.domain
              : DEFAULT_COOKIE_DOMAIN,
          path: '/',
          secure: true,
          httpOnly: false,
        })
        .then(() => {
          injected += 1;
        })
        .catch(() => {
          sendLine({
            kind: 'event',
            params: {
              type: 'error',
              message: `cookie injection failed for ${cookie.name}`,
            },
          });
        }),
    );
  }
  return Promise.all(attempts).then(() => {
    process.stderr.write(
      `sitrus-engine: apple session injected into the Electron profile (${injected} cookies, media-user-token ${
        appleSession.mediaUserToken ? 'present' : 'absent'
      })\n`,
    );
  });
}

// ————— Probe (diagnóstico: cookies observados no perfil) —————

async function resolveSessionProbe(id) {
  // O probe só faz sentido quando a ponte terminou (cookies injetados) e a
  // janela existir; se o app morrer antes, o electron-bridge resolve os
  // pendimentos no 'exit'.
  await sessionReady;
  await windowReady;
  if (!window) {
    sendLine({
      kind: 'response',
      id,
      result: null,
      error: { code: -32000, message: 'electron window is gone' },
    });
    return;
  }
  try {
    const cookies = await session.defaultSession.cookies.get({
      url: COOKIE_TARGET_URL,
    });
    sendLine({
      kind: 'response',
      id,
      result: {
        cookies: cookies.map((cookie) => ({
          name: cookie.name,
          value: cookie.value,
          domain: cookie.domain,
          path: cookie.path,
        })),
        mediaUserToken: appleSession && appleSession.mediaUserToken
          ? appleSession.mediaUserToken
          : null,
      },
    });
  } catch {
    sendLine({
      kind: 'response',
      id,
      result: null,
      error: { code: -32603, message: 'session_probe failed inside electron' },
    });
  }
}

// ————— Canal com o sidecar —————

function sendLine(object) {
  process.stdout.write(`${JSON.stringify(object)}\n`);
}

ipcMain.on('sitrus:renderer-ready', () => {
  rendererReady = true;
  flushQueue();
});

ipcMain.on('sitrus:response', (_event, payload = {}) => {
  const { id, result, error } = payload;
  if (id != null) {
    sendLine({ kind: 'response', id, result: result ?? null, error: error ?? null });
  }
});

ipcMain.on('sitrus:event', (_event, params) => {
  if (params && typeof params === 'object') {
    sendLine({ kind: 'event', params });
  }
});

function flushQueue() {
  while (rendererReady && window && queued.length > 0) {
    const message = queued.shift();
    window.webContents.send('sitrus:command', message);
  }
}

function forward(message) {
  if (rendererReady && window) {
    window.webContents.send('sitrus:command', message);
  } else {
    queued.push(message);
  }
}

readline
  .createInterface({ input: process.stdin, terminal: false })
  .on('line', (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message && message.id != null && typeof message.method === 'string') {
      if (message.method === 'session_probe') {
        resolveSessionProbe(message.id);
        return;
      }
      forward(message);
    }
  })
  .on('close', () => {
    // EOF no stdin (sidecar morto ou disposo): sair com elegância e, se o
    // encerramento do Chromium ficar preso, sair à força — o teto garante a
    // morte do processo mesmo que o app.quit() trave em teardown.
    app.quit();
    setTimeout(() => {
      process.exit(0);
    }, 3000).unref();
  });

// Injeta a fonte do adapter no MAIN world da página (com guardas internas
// contra dupla execução; re-executável a cada navegação).
function attachPageAdapter(targetWindow) {
  if (!PAGE_ADAPTER_SOURCE) return;
  targetWindow.webContents.on('dom-ready', () => {
    targetWindow.webContents
      .executeJavaScript(PAGE_ADAPTER_SOURCE, true)
      .catch((error) => {
        sendLine({
          kind: 'event',
          params: {
            type: 'error',
            message: `page adapter failed to inject: ${error.message}`,
          },
        });
      });
  });
}

app.whenReady().then(async () => {
  // Ponte de sessão: cookies injetados ANTES de qualquer loadURL (e antes do
  // CDM, que pode demorar no primeiro uso).
  try {
    const injected = injectAppleSession();
    sessionReady = injected;
    await injected;
  } catch {
    // consume/injection falhou: segue sem sessão (erros foram reportados).
  }
  resolveSessionReady();

  // ECS v45: o Widevine CDM (4.10.3112.0 para o build 45.2.1+wvcus) é
  // instalado/atualizado em runtime; esperar here é o fluxo recomendado.
  try {
    await components.whenReady();
    process.stderr.write(`sitrus-engine: components ready: ${components.status()}\n`);
  } catch (error) {
    process.stderr.write(`sitrus-engine: components failed: ${error.message}\n`);
    sendLine({
      kind: 'event',
      params: { type: 'error', message: `widevine components failed: ${error.message}` },
    });
  }

  window = new BrowserWindow({
    width: 1280,
    height: 800,
    show: SHOW,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  window.on('closed', () => {
    window = null;
  });
  window.webContents.on('render-process-gone', (_event, details) => {
    sendLine({
      kind: 'event',
      params: { type: 'error', message: `renderer gone: ${details.reason}` },
    });
  });
  if (USE_PAGE_ADAPTER) {
    attachPageAdapter(window);
  }
  resolveWindowReady();

  const load = PAGE.startsWith('http')
    ? window.loadURL(PAGE)
    // A página mock recebe o tick por query para manter o contrato idêntico.
    : window.loadFile(PAGE, { search: `tickMs=${TICK_MS}` });
  load.catch((error) => {
    sendLine({
      kind: 'event',
      params: { type: 'error', message: `page load failed: ${error.message}` },
    });
  });
});

app.on('window-all-closed', () => {
  app.quit();
});
