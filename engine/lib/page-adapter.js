'use strict';
// page-adapter.js — adapter in-page mínimo do player web da Apple Music.
//
// RODA NO MAIN WORLD da página: `main.js` injeta esta fonte via
// `webContents.executeJavaScript` (em apple e apple-mock) — conversa com a
// bridge pelo `window.sitrusEngine` (contextBridge do preload).
//
// Contrato (v1):
//   Detecção de controle
//     • Procura `window.MusicKit.getInstance()` com `addEventListener` e
//       `play` até 4 s (polling); MusicKit é a superfície de controle do
//       player web da Apple.
//     • Sem controle exposto → erro tipado -32002
//       (`RPC_PAGE_CONTROL_UNAVAILABLE`, const do Rust `rpc`): mensagem
//       `page-control-unavailable: …` — resposta por comando E evento
//       `{type:'error'}` uma única vez.
//   Eventos entregues à bridge (emitEvent → WS → Rust):
//     • `playbackStateDidChange` (event.state string) →
//       `{type:'state', value:'playing'|'paused'|'stopped'}`;
//       loading/waiting/stalled/seeking → `{type:'state', value:'loading'}`;
//       'ended'/'completed' → `{type:'ended'}` (página avança a fila; Rust
//       consome o Ended como no MockEngine).
//     • `playbackTimeDidChange` (event.state object) →
//       `{type:'position', ms}` sempre; `{type:'duration', ms}` quando a
//       duração muda (currentTime/duration em segundos, arredondados a ms).
//   Comandos aceitos da bridge (respond com null quando aceitos; falha de
//   execução vira evento de erro — paridade com o MockPlayer mock):
//     • load {items:[{id,...}]} → MusicKit#setQueue({songs:[ids]});
//     • play → MusicKit#play(); pause → MusicKit#pause();
//     • seek {positionMs} → MusicKit#seekToTime(ms/1000);
//     • volume {volume} → MusicKit#volume = v (0.0..=1.0);
//     • desconhecido → resposta de erro -32601.
//
// O adapter é idempotente por mundo (guarda em window) e re-injetável a cada
// navegação de página inteira.
//
// ESTADO DE PROVA: exercitado com a página apple-mock (facade MusicKit em
// mock/apple-mock-page.js). A página REAL music.apple.com continua sem
// validação nesta estação (sem sessão Apple) — ver engine/README.md.

(function attachSitrusPageAdapter() {
  const bridge = window.sitrusEngine;
  if (!bridge || window.__SITRUS_PAGE_ADAPTER__) return;
  window.__SITRUS_PAGE_ADAPTER__ = true;

  const HUNT_MS = 4000; // janela para achar músicakit (< rpc_timeout default)
  const POLL_MS = 100;
  const PAGE_CONTROL_UNAVAILABLE_CODE = -32002;

  let unavailability = null; // erro tipado quando a página não expõe controle
  let instance = null;
  let lastStatus = null;
  let lastDurationMs = null;
  let boundListeners = [];
  let pendingCommands = []; // comandos que aguardam o binding

  function emitError(message) {
    try {
      bridge.emitEvent({ type: 'error', message });
    } catch {
      // bridge indisponível; nada a fazer no mundo da página
    }
  }

  function pageControlUnavailable() {
    return {
      code: PAGE_CONTROL_UNAVAILABLE_CODE,
      message:
        'page-control-unavailable: a página não expõe um player controlável (MusicKit ausente)',
    };
  }

  function probeInstance() {
    try {
      const kit = window.MusicKit;
      if (!kit || typeof kit.getInstance !== 'function') return null;
      const candidate = kit.getInstance();
      if (
        candidate &&
        typeof candidate.play === 'function' &&
        typeof candidate.pause === 'function' &&
        typeof candidate.addEventListener === 'function'
      ) {
        return candidate;
      }
    } catch {
      // página pode ainda estar montando o MusicKit
    }
    return null;
  }

  function onStatusChange(event) {
    const status = event && typeof event.state === 'string' ? event.state : null;
    if (!status || status === lastStatus) return;
    lastStatus = status;
    switch (status) {
      case 'playing':
        bridge.emitEvent({ type: 'state', value: 'playing' });
        return;
      case 'paused':
        bridge.emitEvent({ type: 'state', value: 'paused' });
        return;
      case 'ended':
      case 'completed':
        bridge.emitEvent({ type: 'ended' });
        return;
      case 'stopped':
        bridge.emitEvent({ type: 'state', value: 'stopped' });
        return;
      default:
        // loading/waiting/stalled/seeking → Loading.
        bridge.emitEvent({ type: 'state', value: 'loading' });
    }
  }

  function onTimeChange(event) {
    const time = event && event.state && typeof event.state === 'object' ? event.state : null;
    if (!time) return;
    if (typeof time.currentTime === 'number' && Number.isFinite(time.currentTime)) {
      bridge.emitEvent({
        type: 'position',
        ms: Math.round(time.currentTime * 1000),
      });
    }
    const durationSeconds = typeof time.duration === 'number' ? time.duration : null;
    if (
      durationSeconds != null &&
      Number.isFinite(durationSeconds) &&
      durationSeconds > 0
    ) {
      const durationMs = Math.round(durationSeconds * 1000);
      if (durationMs !== lastDurationMs) {
        lastDurationMs = durationMs;
        bridge.emitEvent({ type: 'duration', ms: durationMs });
      }
    }
  }

  function bindInstance(found) {
    instance = found;
    boundListeners = [];
    const names = [
      'playbackStateDidChange',
      'playbackTimeDidChange',
    ];
    for (const name of names) {
      const handler = name === 'playbackStateDidChange' ? onStatusChange : onTimeChange;
      try {
        instance.addEventListener(name, handler);
        boundListeners.push({ name, handler });
      } catch {
        emitError(`page adapter could not subscribe ${name}`);
      }
    }
  }

  function executeCommand(command) {
    const { id, method, params = {} } = command;
    if (method === undefined || method === '') {
      bridge.respond(id, null, { code: -32602, message: 'page adapter: empty method' });
      return;
    }
    // Caça em curso: o comando espera o binding (a caça termina ≤4 s, antes
    // do rpc_timeout de 5 s do lado Rust). Sem controle após a caça → erro
    // tipado -32002 imediato.
    if (!instance && !unavailability) {
      pendingCommands.push(command);
      return;
    }
    // Falha tipada quando a página não expõe controle (erro imediato, sem
    // segurar o RPC até o timeout do lado do Rust).
    if (!instance) {
      bridge.respond(id, null, unavailability ?? pageControlUnavailable());
      return;
    }
    try {
      switch (method) {
        case 'load': {
          const items = Array.isArray(params.items) ? params.items : [];
          const songs = items.map((item) => item && item.id).filter(Boolean);
          Promise.resolve(instance.setQueue({ songs }))
            .then(() => bridge.respond(id, null))
            .catch((error) => {
              emitError(`page load failed: ${error && error.message}`);
              bridge.respond(id, null);
            });
          return;
        }
        case 'play':
          Promise.resolve(instance.play())
            .then(() => bridge.respond(id, null))
            .catch((error) => {
              emitError(`page play failed: ${error && error.message}`);
              bridge.respond(id, null);
            });
          return;
        case 'pause':
          Promise.resolve(instance.pause())
            .then(() => bridge.respond(id, null))
            .catch((error) => {
              emitError(`page pause failed: ${error && error.message}`);
              bridge.respond(id, null);
            });
          return;
        case 'seek': {
          const targetMs = Number(params.positionMs) || 0;
          Promise.resolve(instance.seekToTime(targetMs / 1000))
            .then(() => bridge.respond(id, null))
            .catch((error) => {
              emitError(`page seek failed: ${error && error.message}`);
              bridge.respond(id, null);
            });
          return;
        }
        case 'volume': {
          const volume = Number(params.volume);
          if (!Number.isFinite(volume) || volume < 0 || volume > 1) {
            // Paridade com o MockPlayer: volume fora de 0.0..=1.0 é evento.
            emitError(`page volume must be within 0.0..=1.0: ${params.volume}`);
            bridge.respond(id, null);
            return;
          }
          Promise.resolve(
            typeof instance.setVolume === 'function'
              ? instance.setVolume(volume)
              : // MusicKit expõe `volume` como propriedade ajustável.
                (instance.volume = volume),
          )
            .then(() => bridge.respond(id, null))
            .catch((error) => {
              emitError(`page volume failed: ${error && error.message}`);
              bridge.respond(id, null);
            });
          return;
        }
        default:
          bridge.respond(id, null, {
            code: -32601,
            message: `unknown method: ${String(method)}`,
          });
      }
    } catch (error) {
      emitError(`page adapter internal error: ${error && error.message}`);
      bridge.respond(id, null);
    }
  }

  // ————— arranque —————

  bridge.onCommand(executeCommand);
  bridge.ready();

  // Busca pelo MusicKit: resolve pendências e ativa eventos; sem MusicKit
  // dentro de HUNT_MS, erro tipado (uma vez) para pendentes e seguintes.
  const startedAt = Date.now();
  const hunter = setInterval(() => {
    const found = probeInstance();
    if (found) {
      clearInterval(hunter);
      bindInstance(found);
      const pending = pendingCommands;
      pendingCommands = [];
      for (const command of pending) {
        executeCommand(command);
      }
      return;
    }
    if (Date.now() - startedAt >= HUNT_MS) {
      clearInterval(hunter);
      unavailability = pageControlUnavailable();
      emitError(unavailability.message);
      const pending = pendingCommands;
      pendingCommands = [];
      for (const command of pending) {
        executeCommand(command);
      }
    }
  }, POLL_MS);
})();
