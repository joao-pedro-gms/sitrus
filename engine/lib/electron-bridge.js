'use strict';
// electron-bridge.js — sobe o Electron castlabs como filho e faz a ponte
// stdin/stdout (JSON lines) com o main.js:
//   comandos  → write em stdin  |  respostas {kind:"response"} ← stdout
//   eventos   → emit(params)    |  notificações {kind:"event"}   ← stdout
// O backend nunca vê o token: ele é removido do env do filho. A sessão Apple
// chega por `sessionFile` (caminho de um arquivo temporário 0600 criado pelo
// sidecar.js): o Electron lê, apaga e injeta os cookies antes do loadURL.
// O VALOR nunca passa por env/argv — só o caminho do arquivo 0600.

const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

function discoverElectronBinary(engineDir) {
  // 1) o main do pacote normalmente exporta o caminho do binário.
  try {
    const entry = require.resolve('@castlabs/electron-releases', {
      paths: [engineDir],
    });
    const exported = require(entry);
    if (typeof exported === 'string' && fs.existsSync(exported)) {
      return exported;
    }
  } catch {
    // segue para as convenções de layout do pacote electron
  }
  // 2) path.txt do pacote electron aponta para o executável relativo.
  const pkgDir = path.join(
    engineDir,
    'node_modules',
    '@castlabs',
    'electron-releases',
  );
  try {
    const distPath = fs.readFileSync(path.join(pkgDir, 'path.txt'), 'utf8').trim();
    const candidate = path.join(pkgDir, distPath);
    if (fs.existsSync(candidate)) return candidate;
  } catch {
    // sem path.txt
  }
  // 3) layout padrão no linux.
  const candidate = path.join(pkgDir, 'dist', 'electron');
  if (fs.existsSync(candidate)) return candidate;
  throw new Error(
    `electron castlabs not installed in ${engineDir}; run: npm ci --prefix ${engineDir}`,
  );
}

function startElectronBridge({ engineDir, emit, sessionFile = null }) {
  const electron = discoverElectronBinary(engineDir);
  const env = { ...process.env };
  delete env.SITRUS_SIDECAR_TOKEN; // o Electron não precisa do token
  delete env.SITRUS_SIDECAR_ELECTRON_ARGS_JSON;
  if (sessionFile) {
    // Só o CAMINHO do arquivo temporário 0600 (nunca o conteúdo).
    env.SITRUS_SESSION_FILE = sessionFile;
  }
  let extraArgs = [];
  const rawArgs = process.env.SITRUS_SIDECAR_ELECTRON_ARGS_JSON;
  if (rawArgs) {
    try {
      extraArgs = JSON.parse(rawArgs);
    } catch {
      extraArgs = [];
    }
  }

  const child = spawn(
    electron,
    [path.join(engineDir, 'main.js'), ...extraArgs],
    { cwd: engineDir, env, stdio: ['pipe', 'pipe', 'inherit'] },
  );

  const pending = new Map();
  let nextId = 1;

  const lines = readline.createInterface({ input: child.stdout });
  lines.on('line', (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.kind === 'response' && message.id != null) {
      const resolve = pending.get(message.id);
      if (resolve) {
        pending.delete(message.id);
        resolve(message);
      }
      return;
    }
    if (message.kind === 'event' && message.params) {
      emit(message.params);
    }
  });

  child.on('exit', (code, signal) => {
    const detail = `electron sidecar exited (code=${code ?? 'null'}${signal ? `, signal=${signal}` : ''})`;
    for (const resolve of pending.values()) {
      resolve({
        kind: 'response',
        error: { code: -32000, message: detail },
      });
    }
    pending.clear();
    emit({ type: 'error', message: detail });
  });

  child.stdin.on('error', () => {
    // escrita após fecho do filho: os pendentes são resolvidos no 'exit'
  });

  function request(method, params) {
    return new Promise((resolve) => {
      const id = nextId;
      nextId += 1;
      pending.set(id, resolve);
      child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
    });
  }

  return {
    mode: 'electron',
    async handle(method, params, reply) {
      // shutdown é do bridge: fecha o main do Electron com elegância
      // (stdin end → readline close → app.quit); a página não o vê.
      if (method === 'shutdown') {
        reply(null);
        try {
          child.stdin.end();
        } catch {
          // best-effort
        }
        return;
      }
      try {
        const response = await request(method, params);
        if (response && response.error) {
          reply({
            code: response.error.code ?? -32000,
            message: String(response.error.message ?? 'electron command failed'),
          });
        } else {
          reply(null, response ? response.result : null);
        }
      } catch (error) {
        reply({
          code: -32603,
          message: `electron bridge error: ${error.message}`,
        });
      }
    },
    dispose() {
      // Encerra o filho e devolve um CONTRATO de morte real: a promessa
      // resolve só quando o processo morreu (grace por stdio EOF; SIGKILL
      // em 400 ms se não tiver respondido). O sidecar aguarda esta promessa
      // antes de sair — sem ela, um `process.exit` do node deixa o Electron
      // órfão se o encerramento do Chromium demorar.
      const exited = new Promise((resolve) => {
        if (child.exitCode !== null || child.signalCode !== null) {
          resolve();
          return;
        }
        child.on('exit', () => resolve());
      });
      try {
        child.stdin.end();
      } catch {
        // best-effort
      }
      // Reforço de limpeza: normalmente o main.js já apagou o arquivo após
      // a leitura; se o Electron morreu antes, ele não sobra.
      if (sessionFile) {
        try {
          fs.unlinkSync(sessionFile);
        } catch {
          // já apagado
        }
      }
      setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) {
          child.kill('SIGKILL');
        }
      }, 400);
      return exited;
    },
  };
}

module.exports = { startElectronBridge };
