# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
e este projeto adere ao [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [0.2.0] - Unreleased

Segunda versão pública: engine DRM sidecar, telas de artista/álbum/playlist e
busca multi-tipo.

### Adicionado

- Engine DRM (`crates/sitrus-engine` + sidecar `engine/`): trait `Engine` em Rust
  (`load_queue`, `play`, `pause`, `seek`, `set_volume`, `state`, `events`) com
  cliente de sidecar por JSON-RPC 2.0 sobre WebSocket restrito a `127.0.0.1`
  (porta efêmera, token de 32 bytes por execução, handshake `initialize`,
  single-client). Sidecar em Node puro com dois modos: mock determinístico e
  Electron castlabs (`@castlabs/electron-releases` pinado em `v45.2.1+wvcus`,
  CDM Widevine `4.10.3112.0` instalado em runtime em
  `~/.config/sitrus-engine/WidevineCdm/`, nenhum binário redistribuído).
- Erros tipados do engine com política de fallback: `EngineError::requires_native_fallback()`
  sinaliza quando cair no `sitrus-player` nativo (sidecar indisponível, handshake
  rejeitado, protocolo/conexão quebrados); falhas `Rpc` e entrada inválida não
  pedem fallback.
- `MockEngine` determinística e suíte de contrato compartilhada rodando contra o
  mock e contra o sidecar real; testes e2e de Electron marcados como `ignored`.
- Páginas de detalhe: artista (abas "Álbuns"/"Músicas", hero com gradiente do
  tema), álbum e playlist (catálogo e biblioteca), com estados de carregamento,
  erro com "Tentar de novo" e estados vazios; pilha de navegação com volta por
  `Escape`, `←` nos detalhes e botão "Voltar".
- Busca multi-tipo no catálogo (`types=artists,albums,songs`) com abas
  Artistas/Álbuns/Faixas e fallback para o filtro local da biblioteca em falha
  de HTTP/JSON.
- Montagem de fila a partir de coleções (artista, álbum, playlist, busca) com
  cursor na faixa clicada (`build_queue`) e reprodução das prévias do catálogo
  (`previewUrl`) pelo player nativo; clique em playlist abre a página dela.
- Fila interativa no painel "Tocando a seguir": clique toca a faixa
  (`move_to`), arraste reordena (`reorder`), com validação de índices.
- Componentes novos em `sitrus-ui`: `DetailHeader`, `TrackTable` e `Tabs`,
  cobertos na galeria.
- Empacotamento: o RPM instala as fontes JS do sidecar em
  `%{_libexecdir}/sitrus/engine` com `Recommends: nodejs`; o AppImage copia o
  mesmo conjunto para `usr/lib/sitrus/engine`. Nenhum `node_modules`, dist do
  Electron ou CDM entra nos pacotes (o bootstrap baixa o dist em runtime).
- `docs/spike-eme.md`: spike de EME no WebKitGTK com veredito `EME_UNSUPPORTED`
  (evidência de CI) — motiva o caminho do sidecar Electron castlabs.

### Limitações conhecidas desta versão

- O app roteia por faixa entre dois backends: o `sitrus-player` nativo (prévias
  do catálogo, arquivos locais e streams HLS/AAC) e o engine DRM integrado —
  faixas sem fonte nativa (por exemplo, as da sua biblioteca) vão ao engine
  quando disponível, com fallback tipado
  (`EngineError::requires_native_fallback()`).
- A **ponte de sessão está em correção**: os cookies e o `media-user-token` da
  sessão do app ainda não são repassados ao perfil do Electron, então o engine
  não tem sessão autorizada para licenciar DRM de faixas da sua conta.
- O controle da página real `music.apple.com` pelo sidecar (adapter de
  play/pause/seek do player web) ainda não foi validado; o controller emite
  eventos de erro honestos nesse caminho.
- O licenciamento Widevine real (VMP/UAT/produção) não foi exercitado — o CDM
  instala e o ciclo de protocolo foi provado com a página mock.
  O engine exige Node no sistema; o dist do Electron (~274 MB) é baixado no
  primeiro uso — nenhum binário Electron/CDM vai nos pacotes.
- O AppImage 0.2.0 ainda não foi construído/medido nesta estação.

## [0.1.0] - Unreleased

Primeira versão pública.

### Adicionado

- Login embutido no Apple Music via webview (WebKitGTK), com captura de cookies e
  `media-user-token` persistidos localmente em arquivo com permissão `0600`.
- Biblioteca: listagem das músicas e playlists da conta do usuário, como navegação
  (prateleiras horizontais e modo lista). Artistas e álbuns ainda não têm tela
  própria; abrir o conteúdo de uma playlist ainda não é suportado.
- Player: play/pause, próxima/anterior, seek e volume; o motor de áudio toca
  arquivos locais e streams HLS/AAC (a reprodução de faixas do catálogo da Apple
  fica para uma versão futura). A fila é somente exibição, sem reordenação.
- Interface nativa egui/eframe com design system próprio (tokens, temas claro e
  escuro, fonte Inter embutida, ícones Lucide).
- Modo lista para seções grandes, além das prateleiras horizontais.
- Build padrão sem webview (`cargo build --release`) e variante com login embutido
  (`--features webview`).
- Empacotamento: AppImage e RPM x86_64 publicados por tag `v*` com `SHA256SUMS`.
- CI em todo push/PR: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test`, `cargo build --release`, actionlint e cargo-deny.
