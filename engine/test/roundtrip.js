'use strict';
// test/roundtrip.js — round-trip de protocolo em Node puro: sobe sidecar.js
// em modo mock, conecta um cliente WebSocket nativo (Node >= 22), autentica
// com token de execução, executa comandos e valida o transcript de eventos.
// Sai 0 quando o ciclo completo responde como o contrato exige; 1 caso contrário.

const { spawn } = require('node:child_process');
const path = require('node:path');

const TOKEN = `rt-${Math.random().toString(16).slice(2)}`;
const READY_PREFIX = 'SITRUS_SIDECAR_READY ';
const TICK_MS = 10;
const ITEM_MS = 300;

const transcript = [];
function log(kind, detail) {
  transcript.push(`${kind} ${detail}`);
  process.stdout.write(`${kind} ${detail}\n`);
}

// Cursor sequencial sobre os eventos já consumidos pelas esperas (os eventos
// podem chegar antes da resposta do comando que os causou).
let eventCursor = 0;

const child = spawn(process.execPath, [path.join(__dirname, '..', 'sidecar.js')], {
  cwd: path.join(__dirname, '..'),
  env: {
    ...process.env,
    SITRUS_SIDECAR_TOKEN: TOKEN,
    SITRUS_SIDECAR_MODE: 'mock',
    SITRUS_SIDECAR_TICK_MS: String(TICK_MS),
  },
  stdio: ['pipe', 'pipe', 'inherit'],
});

let nextId = 1;
const pending = new Map();
const events = [];
let ws = null;
let readyPort = null;
let stdoutBuffer = '';

child.stdout.on('data', (chunk) => {
  stdoutBuffer += chunk.toString();
  let newline;
  while ((newline = stdoutBuffer.indexOf('\n')) >= 0) {
    const line = stdoutBuffer.slice(0, newline).trim();
    stdoutBuffer = stdoutBuffer.slice(newline + 1);
    if (line.startsWith(READY_PREFIX)) {
      const payload = JSON.parse(line.slice(READY_PREFIX.length));
      readyPort = payload.port;
      log('READY', JSON.stringify(payload));
      openSocket(payload.port);
    }
  }
});

child.on('exit', (code) => {
  if (code !== 0 && code !== null) {
    log('EXIT', `sidecar exited with ${code}`);
  }
});

function openSocket(port) {
  ws = new WebSocket(`ws://127.0.0.1:${port}/`);
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data.toString());
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
  ws.onopen = async () => {
    try {
      await run();
      finish(checks());
    } catch (error) {
      finish(false, error.message);
    }
  };
}

function request(method, params) {
  const id = nextId;
  nextId += 1;
  const frame = { jsonrpc: '2.0', id, method, params };
  log('REQUEST', JSON.stringify(frame));
  return new Promise((resolve) => {
    pending.set(id, resolve);
    ws.send(JSON.stringify(frame));
  });
}

function waitForEvent(type, timeoutMs = 5000) {
  return waitForEventWhere(type, () => true, timeoutMs);
}

// Espera um evento que case com tipo + predicate a partir do cursor; eventos
// anteriores ficam marcados como consumidos para os consumidores seguintes.
function waitForEventWhere(type, predicate, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${type}`)), timeoutMs);
    const poll = setInterval(() => {
      const index = events
        .slice(eventCursor)
        .findIndex((item) => item.type === type && predicate(item));
      if (index >= 0) {
        clearInterval(poll);
        clearTimeout(timer);
        eventCursor += index + 1;
        resolve(events[eventCursor - 1]);
      }
    }, 10);
  });
}

async function run() {
  // Autenticação: token válido é recusado se errado, aceito se correto.
  const init = await request('initialize', { protocol: 1, token: TOKEN });
  if (init.error) throw new Error(`initialize rejected: ${init.error.message}`);

  // 1) play sem fila → evento de erro (paridade com o contrato Rust).
  await request('play', {});
  const emptyQueueError = await waitForEvent('error');
  if (!/no queue item is loaded/.test(emptyQueueError.message)) {
    throw new Error(`unexpected empty-queue error: ${emptyQueueError.message}`);
  }

  // 2) load com duas faixas simuladas de 300 ms.
  await request('load', {
    items: [
      { id: 'rt-1', title: null, durationMs: ITEM_MS, url: null },
      { id: 'rt-2', title: null, durationMs: ITEM_MS, url: null },
    ],
  });
  await waitForEvent('duration');

  // 3) play → posições crescentes até Ended.
  await request('play', {});
  await waitForEvent('state'); // playing
  const positionOne = (await waitForEvent('position')).ms;
  const positionTwo = (await waitForEvent('position')).ms;
  if (!(positionTwo > positionOne)) {
    throw new Error(`position must advance: ${positionOne} -> ${positionTwo}`);
  }

  // 4) seek e volume.
  await request('pause', {});
  await request('seek', { positionMs: 100 });
  const seeked = await waitForEventWhere('position', (item) => item.ms === 100);
  log('ASSERT', `seek landed at ${seeked.ms} ms`);
  await request('volume', { volume: 0.5 });

  // 5) retomar e tocar até o fim; a fila avança para a segunda faixa.
  await request('play', {});
  const endedOne = await waitForEvent('ended');
  log('ASSERT', `ended item-1 at ${endedOne.type}`);
  const nextDuration = await waitForEvent('duration');
  if (nextDuration.ms !== ITEM_MS) throw new Error(`advance duration mismatch: ${nextDuration.ms}`);
  await request('play', {});
  const endedTwo = await waitForEvent('ended');
  if (!endedTwo) throw new Error('item-2 did not end');

  // 6) shutdown elegante: resposta e fecho do WebSocket.
  await request('shutdown', {});
}

function checks() {
  const types = events.map((item) => item.type);
  return (
    types.includes('error') &&
    types.includes('duration') &&
    types.includes('state') &&
    types.includes('ended') &&
    events.some((item) => item.type === 'position' && item.ms >= ITEM_MS)
  );
}

function finish(ok, reason) {
  const summary =
    'SUMMARY ' +
    JSON.stringify({ ok: Boolean(ok), reason: reason ?? null, events: events.length });
  log(summary.slice(0, 'SUMMARY'.length), summary.slice('SUMMARY '.length));
  try {
    ws && ws.close();
  } catch {
    // socket já fechado
  }
  // Dá tempo ao sidecar de encerrar via stdin EOF, então derruba o processo.
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
  }, 300);
}

setTimeout(() => finish(false, 'global timeout'), 15_000);
