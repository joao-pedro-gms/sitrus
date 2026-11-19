'use strict';
// apple-mock-page.js — facade do subconjunto MusicKit que o adapter in-page
// (lib/page-adapter.js) consome: window.MusicKit.getInstance() com
// addEventListener, play/pause/seekToTime/setQueue/volume, e os eventos
// 'playbackStateDidChange' / 'playbackTimeDidChange' no mesmo formato que a
// página real usa ({type, state}). O player simulado por trás é o
// SitrusMockPlayer (mesmo motor da página mock de protocolo).

(function attachAppleMockFacade() {
  const params = new URLSearchParams(window.location.search);
  // Duração simulada de cada faixa (idêntica ao e2e: 600 ms) e tick 25 ms.
  const SONG_MS = 600;
  const tickMs = Number(params.get('tickMs')) || 25;

  const listeners = {};
  function dispatch(name, detailState) {
    for (const handler of listeners[name] || []) {
      try {
        handler({ type: name, state: detailState });
      } catch {
        // handler do adapter nunca explode a facade
      }
    }
  }

  let lastDurationSeconds = 0;
  const player = new window.SitrusMockPlayer({
    tickMs,
    emit: (mockEvent) => {
      switch (mockEvent.type) {
        case 'state':
          // estados do mock: playing | paused | stopped
          dispatch('playbackStateDidChange', mockEvent.value);
          return;
        case 'ended':
          // MusicKit sinaliza o fim como status 'ended'.
          dispatch('playbackStateDidChange', 'ended');
          return;
        case 'position': {
          const state = {
            currentTime: mockEvent.ms / 1000,
            duration: lastDurationSeconds,
          };
          dispatch('playbackTimeDidChange', state);
          return;
        }
        case 'duration':
          lastDurationSeconds = mockEvent.ms / 1000;
          dispatch('playbackTimeDidChange', {
            currentTime: 0,
            duration: lastDurationSeconds,
          });
          return;
        default:
          // error: o adapter trata fluxos próprios; a facade não emite.
          return;
      }
    },
  });

  const instance = {
    volume: 1,
    play() {
      player.play();
      return Promise.resolve();
    },
    pause() {
      player.pause();
      return Promise.resolve();
    },
    seekToTime(seconds) {
      player.seek((Number(seconds) || 0) * 1000);
      return Promise.resolve();
    },
    setQueue(options = {}) {
      const songs = Array.isArray(options.songs) ? options.songs : [];
      player.load(songs.map((song) => ({ id: String(song), durationMs: SONG_MS })));
      return Promise.resolve();
    },
    addEventListener(name, handler) {
      if (typeof name !== 'string' || typeof handler !== 'function') return;
      (listeners[name] = listeners[name] || []).push(handler);
    },
    removeEventListener(name, handler) {
      const pool = listeners[name] || [];
      listeners[name] = pool.filter((candidate) => candidate !== handler);
    },
  };

  window.MusicKit = {
    Events: {
      playbackStateDidChange: 'playbackStateDidChange',
      playbackTimeDidChange: 'playbackTimeDidChange',
    },
    PlaybackStatus: {
      loading: 'loading',
      playing: 'playing',
      paused: 'paused',
      stopped: 'stopped',
      ended: 'ended',
      seeking: 'seeking',
      waiting: 'waiting',
      stalled: 'stalled',
      completed: 'completed',
    },
    getInstance() {
      return instance;
    },
  };

  const status = document.getElementById('sitrus-apple-mock-status');
  if (status) {
    status.textContent = `sitrus apple-mock facade ready (tick ${tickMs} ms)`;
  }
})();
