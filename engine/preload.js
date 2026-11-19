'use strict';
// preload.js — ponte contextIsolation entre a página e o main process.
// Expõe exatamente o necessário: receber comandos, responder RPC e emitir
// eventos. Sem nodeIntegration no renderer.

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('sitrusEngine', {
  onCommand: (handler) => {
    ipcRenderer.on('sitrus:command', (_event, command) => handler(command));
  },
  respond: (id, result, error) => {
    ipcRenderer.send('sitrus:response', {
      id,
      result: result ?? null,
      error: error ?? null,
    });
  },
  emitEvent: (params) => {
    ipcRenderer.send('sitrus:event', params);
  },
  ready: () => {
    ipcRenderer.send('sitrus:renderer-ready');
  },
});
