# Sidecar do engine DRM (Electron castlabs)

Camada de reprodução com DRM do Sitrus. O `crates/sitrus-engine` (Rust) expõe
a trait `Engine` e conversa com ESTE sidecar por **JSON-RPC 2.0 sobre
WebSocket, restrito a `127.0.0.1`, porta efêmera, com token de sessão gerado
por execução**. O caminho EME no WebKitGTK está descartado
(`docs/spike-eme.md`, veredito `EME_UNSUPPORTED` — nem ClearKey existe lá).

```
sitrus-engine (Rust, trait Engine)  ── SidecarConfig.session (cookies + media-user-token)
   └─ spawn: node engine/sidecar.js          (token via env, sessão via STDIN — nunca em argv/env/URL/log)
        ├─ modo mock: lib/mock-player.js     (simulação, sem Electron/display)
        ├─ modo electron: lib/electron-bridge.js
        │     └─ spawn: dist do Electron castlabs + main.js
        │          ├─ SITRUS_SESSION_FILE    → cookies 0600 temporários, lidos e APAGADOS
        │          ├─ session.defaultSession.cookies.set (ANTES do loadURL)
        │          ├─ components.whenReady() → CDM Widevine pinado
        │          ├─ BrowserWindow (oculta) → music.apple.com (produção)
        │          │                          │ mock/index.html (prova de protocolo)
        │          │                          │ mock/apple-mock.html (facade MusicKit)
        │          └─ lib/page-adapter.js     → adapter in-page (MusicKit)
        └─ session_probe  → sessão observada (echo mock / cookies.get electron)
```

## Ponte de sessão Apple (blocker B1)

O app guarda a sessão em `~/.local/share/sitrus/session.json` (cookies +
`media_user_token`, modo 0600 — `crates/sitrus-api/src/session.rs`). A ponte
leva essa sessão até o perfil do Electron:

1. **Rust → sidecar (stdin JSON)**: `SidecarConfig::session`
   (`SidecarSession { media_user_token, cookies }`) é serializado em UMA linha
   JSON — `{"v":1,"mediaUserToken":"…","cookies":[{"name":"…","value":"…","domain":".apple.com"}]}`
   — e escrita **no stdin do processo do sidecar** logo após o spawn, antes da
   linha de pronto. Nunca em argv, env, URL ou log (`sidecar.rs`, testes em
   `tests/sidecar_session.rs` escaneiam `/proc/<pid>/{cmdline,environ}` e a
   cauda de stderr).
2. **Sidecar → Electron (arquivo temporário 0600)**: o sidecar escreve o
   envelope em `$(os.tmpdir())/sitrus-engine-sessions/session-<pid>-<hex>.json`
   (criado com `mode 0o600` + `chmod` de reforço) e passa **apenas o caminho**
   por env `SITRUS_SESSION_FILE` ao Electron. `main.js` lê, **apaga o arquivo**
   (e o `dispose` do bridge remove se sobrar) antes de qualquer navegação.
3. **Injeção no perfil (antes do loadURL)**: `main.js` injeta cada cookie com
   `session.defaultSession.cookies.set({url: 'https://music.apple.com/',
   name, value, domain, path: '/', secure: true, httpOnly: false})` —
   domínio `music.apple.com` nos ausentes (default `.apple.com`), ANTES de
   `components.whenReady()` e do `loadURL`. `media_user_token` fica em
   memória do main process (não é cookie; é cabeçalho `Media-User-Token` das
   APIs da Apple) e é devolvido pelo probe.
4. **Observação (prova)**: `session_probe` (JSON-RPC) devolve o que o lado do
   sidecar observou — no modo mock, o echo do envelope; no modo electron, A
   SAÍDA REAL de `session.defaultSession.cookies.get({url:
   'https://music.apple.com/'})` mais o `mediaUserToken`. Valores trafegam
   somente neste WS local autenticado; nenhum log os imprime.

Validações (Rust): envelope sem cookies e sem token é rejeitado (`SessionInvalid`,
`EngineErrorKind::InvalidInput`), nome ≤256 B / valor ≤4 KiB ≤64 cookies,
domínio explicitamente vazio é rejeitado; o lado Node revalida a mesma forma
e ignora envelope inválido sem ecoar conteúdo.

Perfil persistente (`~/.config/sitrus-engine`): os cookies injetados são
_session cookies_ (sem `expirationDate`); dados persistentes do perfil
(escritos pela própria Apple em uma sessão real, se existirem) não são
apagados pela ponte — a sessão corrente sobrescreve os mesmos nomes.

## Versões pinadas

- `package.json`: `"@castlabs/electron-releases":
  "git+https://github.com/castlabs/electron-releases.git#v45.2.1+wvcus"`
  (instalado pelo npm como commit `ab2120c6a5d49d412bcb2b7e3f9c9c3e74cb0243`,
  versão do pacote `45.2.1+wvcus` registrado no `package-lock.json`).
  Mesma série que o Sidra 1.3.2 desta estação (`Electron/45.2.1`).
- `ws` `8.23.0` (servidor WS em Node puro, usado nos dois modos).
- CDM Widevine: **`4.10.3112.0`** — idêntico ao `manifest.json` do CDM do
  Sidra (`~/.config/Sidra/WidevineCdm/4.10.3112.0/`) e **confirmado em
  runtime nesta estação**: o first run do sidecar Electron baixou para
  `~/.config/sitrus-engine/WidevineCdm/4.10.3112.0/_platform_specific/linux_x64/libwidevinecdm.so`.
- **Nenhum binário Electron/CDM é commitado ou redistribuído**: o `dist/`
  baixa no bootstrap (`node node_modules/@castlabs/electron-releases/install.js`
  — o `require('@castlabs/electron-releases')` também auto-baixa quando o
  `path.txt` está ausente). O lockfile resolve o pacote por `git+ssh` (forma
  canônica do npm para github), com fallback anônimo `https` feito pelo
  próprio npm no clone.

## Bootstrap

```sh
cd engine
npm ci                                   # deps (ws + castlabs electron pkg)
node node_modules/@castlabs/electron-releases/install.js   # baixa o dist (~274 MB)
node test/roundtrip.js                   # round-trip de protocolo (Node puro, mock)
node test/electron-roundtrip.js          # round-trip electron + SESSÃO + adapter (com display)
```

Ou pelos testes Rust (`node` e `npm ci` na raiz `engine/` precisam existir):

```sh
cargo test -p sitrus-engine                              # suíte padrão (sem Node)
cargo test -p sitrus-engine -- --ignored                 # mock de Node + ambos os modos electron
```

Modo electron com página mock local (sem sessão Apple):

```sh
SITRUS_SIDECAR_MODE=electron SITRUS_SIDECAR_PAGE=mock node sidecar.js
```

## Protocolo (JSON-RPC 2.0 sobre WS)

- Ao subir, o sidecar imprime **uma única linha** em stdout:
  `SITRUS_SIDECAR_READY {"port": <efêmera>}`. Ela só sai depois que o
  backend subiu (e, no modo electron, depois que a sessão foi lida do stdin
  ou do limite de 300 ms sem sessão). Nada mais em stdout; logs vão para
  stderr e **nunca incluem o token ou dados da sessão**.
- Primeira mensagem obrigatória: `{"jsonrpc":"2.0","id":0,"method":"initialize",
  "params":{"protocol":1,"token":"…"}}` — token diferente de `SITRUS_SIDECAR_TOKEN`
  → erro `-32001` + fecho `1008`. Idle de 10 s para autenticar; `Origin`
  presente na conexão → fecho imediato; um único cliente por execução.
- Comandos: `load {items:[QueueItem]}`, `play`, `pause`, `seek {positionMs}`,
  `volume {volume}`, `shutdown`, `session_probe` — em `camelCase` no fio
  (`durationMs`, `positionMs`). `shutdown` é encerrado no bridge (fecha o
  main do Electron via stdio EOF); `session_probe` responde
  `{"cookies":[{"name","value","domain","path"}],"mediaUserToken":"…"}`.
- Push de estado (notificações `{"method":"event","params":…}`):
  `{"type":"state","value":"playing"}`, `{"type":"position","ms":1500}`,
  `{"type":"duration","ms":210000}`, `{"type":"ended"}`, `{"type":"error","message":"…"}`.
  Erros de reprodução vão como eventos (paridade com o `Player` nativo);
  erros RPC ficam para infraestrutura (`-32001` token, `-32002` página sem
  controle, `-32601` método, `-32603` interno, `-32000` bridge).

## Adapter in-page (`lib/page-adapter.js`)

Scripts injetados no **main world** da página por `main.js`
(`webContents.executeJavaScript`, em `apple` e `apple-mock`), guardados por
`window.__SITRUS_PAGE_ADAPTER__` (idempotente por navegação). Contrato v1:

- Detecção: procura `window.MusicKit.getInstance()` com `addEventListener` +
  `play`, polling a cada 100 ms até **4 s** (< `rpc_timeout` de 5 s do lado
  Rust). Enquanto caça, comandos ficam em fila; se a página não expõe
  controle, todos os pendentes e os seguintes recebem o erro tipado
  **`-32002` / mensagem `page-control-unavailable: …`** (`rpc::
  RPC_PAGE_CONTROL_UNAVAILABLE`) e um evento `{"type":"error"}` único.
- Eventos para a bridge (`sitrusEngine.emitEvent`):
  - `playbackStateDidChange` → `{type:'state', value:'playing'|'paused'|'stopped'}` /
    `'loading'` para loading/waiting/stalled/seeking; `'ended'`/`'completed'` →
    `{type:'ended'}` (a página avança a própria fila; o Rust consome).
  - `playbackTimeDidChange` → `{type:'position', ms}` (currentTime em
    segundos, arredondado) e `{type:'duration', ms}` quando a duração muda.
- Comandos: `load {items}` → `MusicKit#setQueue({songs:[ids]})`; `play` →
  `#play()`; `pause` → `#pause()`; `seek {positionMs}` → `#seekToTime(ms/1000)`;
  `volume {volume}` → `#volume` (0.0..=1.0; fora da faixa → evento de erro,
  paridade com o mock). Aceite responde `null`; falha de execução vira
  evento de erro; método desconhecido responde `-32601`.

**Prova executada contra `mock/apple-mock.html` + `mock/apple-mock-page.js`**
(facade MusicKit no mesmo formato de evento da página real, 600 ms/faixa,
tick 25 ms): load → duration 600 → playing → posições crescentes → ended, e
erro tipado `-32002` quando o controle não existe (variante de página sem
MusicKit coberta pelo fluxo do adapter). A página REAL `music.apple.com`
continua sem validação nesta estação (não há sessão Apple real).

## Segurança

- WS escuta **só** em `127.0.0.1` com porta efêmera (`port: 0`); nenhum bind
  em interface externa.
- Token de 32 bytes de `/dev/urandom` (hex) **por execução**, trocado
  exclusivamente na mensagem `initialize` (nunca em URL, argv ou log) e
  passado ao Node por env; o bridge **remove** o token do env do Electron.
- Sessão Apple: valores via **stdin** (Rust→sidecar) e **arquivo 0600
  apagado após a leitura** (sidecar→Electron); **nunca** argv, env, URL ou
  log (grep da revisão + testes de superfície de processo em
  `tests/sidecar_session.rs`); injeção no perfil com
  `session.defaultSession.cookies.set` antes de qualquer `loadURL`.
- Teardown determinístico: EOF no sidecar → `graceful` aguarda a morte REAL
  do Electron (promessa do `dispose` do bridge: gracia por stdio EOF +
  SIGKILL em 400 ms) antes de sair e o main do Electron tem teto de
  3 s pós-`app.quit()` (`process.exit`) — sem órfãos de processo (verificado
  por `/proc/<pid>/exe`).

## Tamanho medido nesta estação (Fedora 45, 2026-11-19)

| Medição | Valor |
| --- | --- |
| `engine/` commitado (fontes JS + package.json/lock, sem `node_modules`) | **112 KB** (`du -sh --exclude=node_modules engine/`; 79.850 B — cresceu de 56 KB → 64 K na revisão com a ponte de sessão, o adapter in-page, a página apple-mock e o teste electron) |
| `engine/node_modules/` (npm ci completo) | **288 MB** |
| — do qual o dist do Electron castlabs baixado (`…/electron-releases/dist`) | **274 MB** |
| Cache do instalador `~/.cache/electron` (zip intermediário, pode limpar) | **354 MB** |
| CDM Widevine baixado em runtime (`~/.config/sitrus-engine/WidevineCdm/`) | **21 MB** (versão `4.10.3112.0`) |

## O que já foi provado localmente vs. o que cabe a CI/sessão Apple

**Provado nesta estação (sem sessão Apple real, com display):**

1. Contrato da trait em Rust contra o `MockEngine` (suíte padrão
   `cargo test -p sitrus-engine`: 25 passed + clippy `-D warnings` verde).
2. Round-trip de protocolo em Node puro (`node test/roundtrip.js`, exit 0,
   71 eventos): initialize com token, load/play/pause/seek/volume, posições
   crescentes, `ended`, avanço de fila, shutdown.
3. **Transporte da sessão sem vazamento** (`cargo test -p sitrus-engine
   --test sidecar_session -- --ignored`): echo do envelope no
   `session_probe` do modo mock + `/proc/<pid>/{cmdline,environ}` do Node
   SEM o valor do cookie e SEM o media-user-token + cauda de stderr limpa.
4. **Ponte de sessão ponta a ponta no modo Electron**
   (`cargo test -p sitrus-engine --test sidecar_e2e -- --ignored
   electron_mode_round_trip_with_session_and_apple_mock_page` e
   `node test/electron-roundtrip.js`): envelope por stdin → arquivo 0600 →
   `cookies.set` antes do `loadURL` → **`cookies.get` lê os cookies
   injetados** (literal no transcript do e2e JS) → adapter in-page aciona o
   facade MusicKit `load → playing → posições → Ended` → teardown sem
   órfãos (arquivo de sessão apagado, `/proc` limpo).
5. Erros tipados de indisponibilidade: Node ausente/engine faltando viram
   `EngineError::SidecarUnavailable` com `requires_native_fallback() == true`;
   envelope inválido vira `SessionInvalid` (sem spawn).
6. Variantes do envelope contra o lado Node (prova única): token-only
   aceito; cookie sem valor rejeitado; campos extras tolerados; `v != 1`
   rejeitado; sessão ausente (EOF/superado o limite) sobe sem sessão.

**Não provado aqui (exige sessão Apple real / CI dedicado):** a página REAL
`music.apple.com` autenticada pelos cookies injetados (não há sessão real
nesta estação — não é possível abrir a página com sessão válida), o
**licenciamento Widevine de verdade** (VMP/UAT/prod: o CDM instalou e o
`components.status()` seguiu verde, mas nenhuma faixa real foi licenciada),
o adapter contra a MusicKit REAL (a facade cobre o subconjunto documentado
do contrato) e o caminho empacotado (RPM/AppImage) com Node presente.

## Arquivos

- Rust: `crates/sitrus-engine/` (Cargo.toml, src/{lib,engine,error,mock}.rs,
  src/rpc/mod.rs — tipos de sessão + `SessionProbe`/`session_probe`,
  src/sidecar.rs — transporte stdin + probe + `process_id`/`stderr_tail`,
  tests/{common/mod,contract,rpc_messages,error_classification,sidecar_e2e,
  sidecar_session}.rs).
- Sidecar JS: `engine/` (package.json, package-lock.json, sidecar.js,
  main.js, preload.js, lib/{mock-player,electron-bridge,page-adapter}.js,
  mock/{index.html,page.js,apple-mock.html,apple-mock-page.js},
  test/{roundtrip,electron-roundtrip}.js, .gitignore, README.md).
