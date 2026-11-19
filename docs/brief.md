# Sitrus — brief de clarificação

Status: fechado (entrevista ulw-interview, 3 rodadas)
Data: 2026-11-19
Decisões: 3/3 dimensões resolvidas (resultado, restrições, critérios de sucesso)

## Pedido original

Cliente nativo de Apple Music para Linux em Rust + egui, com repositório novo no GitHub,
pipelines de qualidade/CI/CD bem definidos e automação de release de instaladores binários
em AppImage e RPM.

## Fatos verificados do ambiente

- `/home/joaopedrogms/Documentos/Projetos/Sitrus`: git iniciado, nenhum commit, nenhum remote.
- rustc/cargo 1.99.1; gh 2.105.0 autenticado como `joao-pedro-gms` (https).

## Objetivo

Um app desktop Linux (janela egui nativa) que entra na conta do usuário na Apple Music, mostra
sua biblioteca e playlists, e reproduz áudio com controles de player completos.

## Escopo do primeiro release (v0.1.0)

Dentro:

- Login embutido: webview aponta para `music.apple.com`, usuário autentica, cookies +
  `media-user-token` são capturados e guardados localmente.
- Biblioteca: artistas, álbuns, músicas do usuário (listagem + navegação).
- Playlists: listar e abrir playlists do usuário, tocar a playlist como fila.
- Player: play/pause, próxima/anterior, seek, volume, fila visível com reordenação.
- Repositório público `sitrus` no GitHub com CI/CD e release de AppImage + RPM.

Fora (não-objetivos explícitos):

- Apple Music oficial/MusicKit e conta Apple Developer.
- macOS, Windows, mobile.
- Letras, favoritos/avaliação, download offline, rádio, recomendações, busca no catálogo global.
- aarch64, assinatura GPG, repositório RPM hospedado, auto-update no app, Flatpak, .deb.
- Sincronismo com a biblioteca local do iTunes / arquivos locais.

## Restrições e decisões

1. **Integração de áudio** — API não oficial do player web (`music.apple.com`) usando os cookies
   da sessão do usuário. Sem conta Apple Developer. Consequências assumidas: risco de ToS e
   quebra quando a Apple mudar o player; portanto o cliente HTTP e o provedor de token precisam
   ficar isolados atrás de uma camada testável com fixtures.
2. **Stack** — Rust (edition 2024), egui/eframe para UI, webview via wry só para login,
   Linux-only, x86_64.
3. **Entrega** — por tag `v*`: AppImage + RPM x86_64, checksums SHA256, notas de release geradas.

## Critérios de aceite (verificáveis)

- CI em todo push/PR: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`,
  `cargo build --release`, lint dos workflows (actionlint), auditoria de dependências (cargo-deny).
- CI falha o merge quando qualquer gate acima falha (branch protection em `main`).
- Tag `v*` gera automaticamente um release com `Sitrus-x86_64.AppImage`, `sitrus-<versão>-1.x86_64.rpm`,
  `SHA256SUMS` e notas de release; nenhum passo manual.
- O AppImage executa em Fedora sem instalar dependências e abre a janela do app.
- O RPM instala via `dnf install ./sitrus-*.rpm` e abre o app pelo menu/`sitrus`.
- Com cookies válidos, o app lista biblioteca e playlists e reproduz uma faixa com seek,
  volume e fila funcionando.
- Sem cookies válidos, o app mostra a tela de login e a mensagem de erro correta, sem panic.

## Risco e ambiguidade remanescente (não bloqueia o plano)

- Backend de áudio (GStreamer x rodio+symphonia) é decisão de execução, resolvida por spike em M2;
  afeta empacotamento (dependências do RPM / plugins no AppImage).
- Endpoints não oficiais podem exigir ajuste na primeira execução real; o plano prevê
  `scripts/probe` para validar contra a conta do usuário.

## Próximo caminho

Plano de entrega em `docs/plan.md` (milestones M0–M4), depois implementação.
