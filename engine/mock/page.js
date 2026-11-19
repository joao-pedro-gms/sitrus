'use strict';
// page.js — página mock (file://): controlador com as mesmas semânticas do
// MockPlayer, provando o ciclo completo comandos→eventos dentro do renderer
// Electron, sem sessão Apple nem DRM. O tick vem pela query (?tickMs=N) que
// o main.js injeta no loadFile.

(function attachMockPage() {
  const api = window.sitrusEngine;
  if (!api || !window.SitrusMockPlayer) return;

  const params = new URLSearchParams(window.location.search);
  const tickMs = Number(params.get('tickMs')) || 100;
  const player = new window.SitrusMockPlayer({
    tickMs,
    emit: (eventParams) => api.emitEvent(eventParams),
  });

  function handle(command) {
    const { id, method, params: commandParams = {} } = command;
    try {
      switch (method) {
        case 'load':
          player.load(Array.isArray(commandParams.items) ? commandParams.items : []);
          break;
        case 'play':
          player.play();
          break;
        case 'pause':
          player.pause();
          break;
        case 'seek':
          player.seek(Number(commandParams.positionMs) || 0);
          break;
        case 'volume':
          player.setVolume(commandParams.volume);
          break;
        default:
          api.respond(id, null, { code: -32601, message: `unknown method: ${method}` });
          return;
      }
      api.respond(id, null);
    } catch (error) {
      api.respond(id, null, { code: -32603, message: `mock page error: ${error.message}` });
    }
  }

  document.getElementById('sitrus-mock-status').textContent =
    `sitrus mock renderer ready (tick ${tickMs} ms)`;
  api.onCommand(handle);
  api.ready();
})();
