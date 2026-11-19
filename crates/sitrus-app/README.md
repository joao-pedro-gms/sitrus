# sitrus-app

Binário do Sitrus: janela eframe/egui com as telas produto (direção B),
integração com `sitrus-api` (biblioteca + sessão), `sitrus-player`
(reprodução) e `sitrus-core` (fila e configuração).

## Rodar

```sh
cargo run -p sitrus-app                  # janela principal
cargo run -p sitrus-app -- --gallery     # galeria do design system (sitrus-ui)
cargo run -p sitrus-app -- --login-manual
cargo run -p sitrus-app -- --version     # -V / --help
```

Requer um display Wayland/X11 (a janela abre com `eframe::run_native`, tema
escuro por padrão e acompanhamento do tema do sistema quando disponível,
janela 1280×820, mínima 980×620).

## Login

- **Manual (sempre disponível)**: copie o cabeçalho `Cookie:` de um pedido a
  `amp-api.music.apple.com` no DevTools do navegador e cole na tela de login
  (`sitrus-app --login-manual` ou a tela "Entrar" que abre sem sessão). A
  sessão é persistida em `~/.local/share/sitrus/session.json` (0600) pelo
  `sitrus-api`.
- **Webview (feature opcional `webview`)**: `sitrus-app` abre
  `music.apple.com` em janela própria (wry/WebKitGTK via tao) e captura a
  sessão quando o media-user-token aparece nos cookies. Compila apenas com:

  ```sh
  cargo build -p sitrus-app --features webview   # exige webkit2gtk-4.1 + dbus-1
  ```

  A build default **não** referencia `wry`/`tao` em nenhum ponto fora da
  feature.

## Atalhos

`Espaço` play/pause · `←/→` seek ±10 s · `↑/↓` volume · `Ctrl+F` busca ·
`Ctrl+Q` fechar janela.

## Estrutura

| módulo | papel |
|---|---|
| `main.rs` | CLI (`--version`, `--gallery`, `--login-manual`) e `eframe::run_native` |
| `app.rs` | Estado do aplicativo, layout (rail 72 px + telas + player bar 88 px), atalhos, toast |
| `net.rs` | Thread de rede: biblioteca, busca de catálogo (fallback local), artwork |
| `player.rs` | Controlador do transporte: `sitrus-player` + `PlaybackQueue` de `sitrus-core` |
| `login.rs` | Tela de login (cookies colados + botão de webview) |
| `webview.rs` | Subprocesso de login por webview — somente `feature = "webview"` |

Toda chamada de rede roda na thread de `net.rs`; a thread da UI só lê eventos
por canal (`mpsc`) — nenhum frame é bloqueado.
