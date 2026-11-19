'use strict';
// mock-player.js — player simulado com as mesmas semânticas do MockEngine
// em Rust (crates/sitrus-engine/src/mock.rs): eventos (state/position/duration/
// ended/error) e avanço de fila pós-Ended. Duplo ambiente: usado como módulo
// Node pelo sidecar em modo mock e como script de browser pela página mock
// (global SitrusMockPlayer).

class MockPlayer {
  constructor({ tickMs = 100, emit }) {
    this.tickMs = clampTick(tickMs);
    this.emit = emit || (() => {});
    this.queue = [];
    this.index = 0;
    this.state = 'stopped';
    this.position = 0;
    this.duration = 0;
    this.volume = 1.0;
    this.timer = setInterval(() => this.tick(), this.tickMs);
  }

  dispose() {
    clearInterval(this.timer);
  }

  // Durância simulada quando o item não declara durationMs (paridade:
  // DEFAULT_SIMULATED_DURATION_MS = 3000 no Rust).
  simulatedDuration(item) {
    const duration = item && Number(item.durationMs);
    return Number.isFinite(duration) && duration > 0 ? duration : 3000;
  }

  transition(next) {
    if (this.state !== next) {
      this.state = next;
      this.emit({ type: 'state', value: next });
    }
  }

  load(items) {
    this.queue = Array.isArray(items) ? items : [];
    this.index = 0;
    this.transition('stopped');
    this.position = 0;
    this.emit({ type: 'position', ms: 0 });
    if (this.queue.length > 0) {
      this.duration = this.simulatedDuration(this.queue[0]);
      this.emit({ type: 'duration', ms: this.duration });
    } else {
      this.duration = 0;
    }
  }

  play() {
    if (this.state === 'playing') return;
    if (this.queue.length === 0) {
      this.emit({ type: 'error', message: 'no queue item is loaded' });
      return;
    }
    this.transition('playing');
    this.emit({ type: 'position', ms: this.position });
  }

  pause() {
    if (this.state !== 'playing') return;
    this.transition('paused');
    this.emit({ type: 'position', ms: this.position });
  }

  seek(positionMs) {
    if (this.queue.length === 0) {
      this.emit({ type: 'error', message: 'no queue item is loaded' });
      return;
    }
    const target = Math.max(0, Math.min(Number(positionMs) || 0, this.duration));
    this.position = target;
    this.emit({ type: 'position', ms: target });
  }

  setVolume(volume) {
    // Paridade com o Rust: NaN e fora de 0.0..=1.0 viram evento de erro.
    if (typeof volume !== 'number' || !(volume >= 0 && volume <= 1)) {
      this.emit({
        type: 'error',
        message: `volume must be within 0.0..=1.0: ${volume}`,
      });
      return;
    }
    this.volume = volume;
  }

  tick() {
    if (this.state !== 'playing') return;
    this.position += this.tickMs;
    if (this.position >= this.duration) {
      this.position = this.duration;
      this.emit({ type: 'position', ms: this.position });
      this.transition('stopped');
      this.emit({ type: 'ended' });
      this.advance();
      return;
    }
    this.emit({ type: 'position', ms: this.position });
  }

  // Ao terminar, avança para o próximo item (posição zero) e aguarda play().
  advance() {
    const next = this.index + 1;
    if (next < this.queue.length) {
      this.index = next;
      this.position = 0;
      this.duration = this.simulatedDuration(this.queue[next]);
      this.emit({ type: 'position', ms: 0 });
      this.emit({ type: 'duration', ms: this.duration });
    }
  }
}

function clampTick(raw) {
  const value = Number(raw);
  if (!Number.isFinite(value) || value < 10 || value > 1000) return 100;
  return value;
}

globalThis.SitrusMockPlayer = MockPlayer;
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { MockPlayer };
}
