# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
e este projeto adere ao [Versionamento Semântico](https://semver.org/lang/pt-BR/).

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
