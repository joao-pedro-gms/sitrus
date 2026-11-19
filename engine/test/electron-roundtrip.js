'use strict';
// test/electron-roundtrip.js — round-trip completo no modo Electron castlabs
// com a ponte de sessão (blocker B1): sobe sidecar.js (modo electron, página
// apple-mock), entrega a sessão Apple pelo STDIN (envelope {v:1, cookies,
// mediaUserToken}), e valida, contra o Electron real:
//   1. session.defaultSession.cookies.get — o probe devolve o LITERAL dos
//      cookies observados no perfil após a injeção antes do loadURL;
//   2. o cookie e o media-user-token não aparecem em /proc (cmdline/environ)
//      de NENHUM processo (node sidecar, Electron e descendentes);
//   3. o adapter in-page aciona o facade MusicKit da página apple-mock:
//      load → state playing → posições crescentes → ended;
//   4. a cauda de stderr do sidecar não contém segredo.
// Sai 0 quando tudo casa; 1 caso contrário. Requer: npm ci --prefix engine
// (dist castlabs instalado) e display (neste repo, SITRUS no Linux X11).

const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');

const TOKEN = `ert-${Math.random().toString(16).slice(2)}`;
const COOKIE_NAME = 'citm';
const COOKIE_VALUE = `SITRUS-ERT-COOKIE-${Math.random().toString(16).slice(2, 10)}`;
const MEDIA_TOKEN = `SITRUS-ERT-MEDIA-TOKEN-${Math.random().toString(16).slice(2, 10)}`;
const READY_PREFIX = 'SITRUS_SIDECAR_READY ';
const TICK_MS = 25;

const transcript = [];
function log(kind, detail) {
  transcript.push(`${kind} ${detail}`);
  process.stdout.write(`${kind} ${detail}\n`);
}

// ————— spawn do sidecar (sessão vai por stdin, NUNCA por argv/env) —————

const child = spawn(process.execPath, [path.join(__dirname, '..', 'sidecar.js')], {
  cwd: path.join(__dirname, '..'),
  env: {
    ...process.env,
    SITRUS_SIDECAR_TOKEN: TOKEN,
    SITRUS_SIDECAR_MODE: 'electron',
    SITRUS_SIDECAR_PAGE: 'apple-mock',
    SITRUS_SIDECAR_TICK_MS: String(TICK_MS),
    SITRUS_SIDECAR_ELECTRON_ARGS_JSON: JSON.stringify(['--no-sandbox']),
    // isolamento do perfil para este round-trip? NÃO: o CDM 4.10.3112.0 já
    // instalado em ~/.config/sitrus-engine é reaproveitado (sem download).
  },
  stdio: ['pipe', 'pipe', 'pipe'],
});
const SESSION = {
  v: 1,
  mediaUserToken: MEDIA_TOKEN,
  cookies: [
    { name: COOKIE_NAME, value: COOKIE_VALUE, domain: '.apple.com' },
    { name: 'geo', value: 'br' },
  ],
};

let stderrChunks = '';
child.stderr.on('data', (chunk) => {
  stderrChunks += chunk.toString();
});

try {
  child.stdin.write(`${JSON.stringify(SESSION)}\n`);
} catch (error) {
  log('STDIN', `session write failed: ${error.message}`);
}

// ————— espera da linha de pronto —————

let nextId = 1;
const pending = new Map();
const events = [];
let ws = null;
let stdoutBuffer = '';
function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function request(method, params, timeoutMs = 10_000) {
  const id = nextId;
  nextId += 1;
  log('REQUEST', JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`timed out waiting for ${method}`));
    }, timeoutMs);
    pending.set(id, (response) => {
      clearTimeout(timer);
      resolve(response);
    });
    ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
  });
}

function waitForEvent(type, predicate, timeoutMs = 10_000) {
  return new Promise((resolve, reject) => {
    if (events.some((item) => item.type === type && (!predicate || predicate(item)))) {
      resolve();
      return;
    }
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${type}`)), timeoutMs);
    const poll = setInterval(() => {
      if (events.some((item) => item.type === type && (!predicate || predicate(item)))) {
        clearInterval(poll);
        clearTimeout(timer);
        resolve();
      }
    }, 15);
  });
}

function scanProcForNeedles(needles) {
  const hits = [];
  const dir = '/proc';
  for (const entry of fs.readdirSync(dir)) {
    if (!/^[0-9]+$/.test(entry)) continue;
    if (String(entry) === String(process.pid)) continue; // este scanner
    for (const surface of ['cmdline', 'environ']) {
      let bytes;
      try {
        bytes = fs.readFileSync(path.join(dir, entry, surface));
      } catch {
        continue; // de outro usuário/encerrado
      }
      const text = bytes.toString('latin1');
      for (const needle of needles) {
        if (text.includes(needle)) {
          hits.push(`pid ${entry} ${surface}`);
        }
      }
    }
  }
  return hits;
}

function finish(ok, reason) {
  const summary =
    'SUMMARY ' +
    JSON.stringify({ ok: Boolean(ok), reason: reason ?? null, events: events.length });
  log(summary.slice(0, 'SUMMARY'.length), summary.slice('SUMMARY '.length));
  try {
    ws && ws.close();
  } catch {
    // já fechado
  }
  try {
    child.stdin.end();
  } catch {
    // best-effort
  }
  setTimeout(() => {
    try {
      child.kill();
    } catch {
      // já saiu
    }
    process.stdout.write(`TRANSCRIPT_LINES ${transcript.length}\n`);
    process.exit(ok ? 0 : 1);
  }, 600);
}

async function run() {
  const init = await request('initialize', { protocol: 1, token: TOKEN });
  if (init.error) throw new Error(`initialize rejected: ${init.error.message}`);

  // (1) PROBE: o literal de session.defaultSession.cookies.get observado no
  //     perfil do Electron — a prova da injeção antes do loadURL.
  const probe = await request('session_probe', {});
  log('PROBE', `cookies.get literal: ${JSON.stringify(probe.result)}`);
  if (probe.error) throw new Error(`session_probe failed: ${probe.error.message}`);
  const cookiesSeen = (probe.result && probe.result.cookies) || [];
  const injected = cookiesSeen.find((cookie) => cookie.name === COOKIE_NAME);
  if (!injected) {
    throw new Error(`cookie ${COOKIE_NAME} not observed by cookies.get: ${JSON.stringify(cookiesSeen)}`);
  }
  if (injected.value !== COOKIE_VALUE) {
    throw new Error(`cookie value mismatch: ${injected.value}`);
  }
  if (injected.domain !== '.apple.com') {
    throw new Error(`cookie domain mismatch: ${injected.domain}`);
  }
  if ((probe.result.mediaUserToken ?? null) !== MEDIA_TOKEN) {
    throw new Error('media user token mismatch');
  }

  // (2) Superfície de processo: segredo fora de cmdline/environ de todos os
  //     pids legíveis (node sidecar, Electron e descendentes).
  const hits = scanProcForNeedles([COOKIE_VALUE, MEDIA_TOKEN]);
  if (hits.length > 0) {
    throw new Error(`secrets leaked into process surfaces: ${hits.join(', ')}`);
  }
  log('ASSERT', 'cookie value and media token absent from /proc cmdline/environ');

  // (3) Ciclo pelo adapter in-page (página apple-mock → facade MusicKit).
  await request('load', { items: [{ id: 'am-facade-1', durationMs: 600, url: null, title: null }] });
  await waitForEvent('duration', (item) => item.ms === 600);
  await request('play', {});
  const playingFrom = events.length;
  await waitForEvent('state', (item) => item.value === 'playing');
  await waitForEvent('ended', null, 10_000);
  const positions = events
    .slice(playingFrom)
    .filter((item) => item.type === 'position')
    .map((item) => item.ms);
  if (!(positions.length >= 2)) {
    throw new Error('expected multiple position events while playing');
  }
  log('ASSERT', `adapter drove playback to ended (positions up to ${Math.max(...positions)} ms)`);

  // (4) Log do sidecar livre de segredos.
  const stderrTail = stderrChunks.split('\n').filter(Boolean).slice(-8).join('\n');
  if (stderrTail.includes(COOKIE_VALUE) || stderrTail.includes(MEDIA_TOKEN)) {
    throw new Error('secrets found in sidecar stderr');
  }

  await request('shutdown', {});
}

function openSocket(port) {
  ws = new WebSocket(`ws://127.0.0.1:${port}/`);
  ws.onmessage = (event) => {
    let message;
    try {
      message = JSON.parse(event.data.toString());
    } catch {
      return;
    }
    if (message.method === 'event') {
      events.push(message.params);
      log('EVENT', JSON.stringify(message.params));
      return;
    }
    if (message.id != null && pending.has(message.id)) {
      const resolve = pending.get(message.id);
      pending.delete(message.id);
      log('RESPONSE', JSON.stringify(message));
      resolve(message);
    }
  };
  ws.onclose = (event) => log('CLOSE', `code=${event.code} reason=${event.reason}`);
  ws.onerror = () => {};
  ws.onopen = () => {
    run()
      .then(() => finish(true))
      .catch((error) => finish(false, error.message));
  };
}

child.stdout.on('data', (chunk) => {
  stdoutBuffer += chunk.toString();
  let newline;
  while ((newline = stdoutBuffer.indexOf('\n')) >= 0) {
    const line = stdoutBuffer.slice(0, newline).trim();
    stdoutBuffer = stdoutBuffer.slice(newline + 1);
    if (line.startsWith(READY_PREFIX)) {
      const payload = JSON.parse(line.slice(READY_PREFIX.length));
      log('READY', JSON.stringify(payload));
      openSocket(payload.port);
    }
  }
});

child.on('exit', (code) => {
  log('EXIT', `sidecar exited with ${code ?? 'null'}`);
});

setTimeout(() => finish(false, 'global timeout'), 90_000);
