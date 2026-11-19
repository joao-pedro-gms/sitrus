# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
e este projeto adere ao [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [0.1.0] - Unreleased

Primeira versão pública.

### Adicionado

- Login embutido no Apple Music via webview (WebKitGTK), com captura de cookies e
  `media-user-token` persistidos localmente em arquivo com permissão `0600`.
- Biblioteca: listagem e navegação por artistas, álbuns e músicas da conta do usuário.
- Playlists: listagem, abertura e reprodução da playlist inteira como fila.
- Player: play/pause, próxima/anterior, seek, volume e fila visível com reordenação.
- Interface nativa egui/eframe com design system próprio (tokens, temas claro e
  escuro, fonte Inter embutida, ícones Lucide).
- Modo lista para seções grandes, além das prateleiras horizontais.
- Build padrão sem webview (`cargo build --release`) e variante com login embutido
  (`--features webview`).
- Empacotamento: AppImage e RPM x86_64 publicados por tag `v*` com `SHA256SUMS`.
- CI em todo push/PR: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test`, `cargo build --release`, actionlint e cargo-deny.
