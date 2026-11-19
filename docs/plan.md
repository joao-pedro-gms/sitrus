# Sitrus — plano de entrega

Deriva de `docs/brief.md` e `docs/design-system.md`. Nada aqui foi executado ainda.

## Estrutura do repositório

```
sitrus/
├── .github/workflows/ci.yml          # fmt, clippy, test, build, actionlint, cargo-deny
├── .github/workflows/release.yml     # tag v* -> AppImage + RPM + SHA256SUMS + notes
├── .github/workflows/audit.yml       # cron semanal: cargo-deny / advisories
├── .github/dependabot.yml
├── crates/
│   ├── sitrus-app/                   # bin: eframe/egui, janela, atalhos, galeria
│   ├── sitrus-core/                  # domínio: biblioteca, playlists, fila, estado do player
│   ├── sitrus-api/                   # cliente HTTP não oficial + provedor de token/cookies
│   ├── sitrus-player/                # backend de áudio (GStreamer ou rodio+symphonia)
│   ├── sitrus-engine/                # engine DRM: trait Engine + cliente do sidecar (v0.2)
│   └── sitrus-ui/                    # tokens, tema, fonte, componentes, galeria
├── engine/                           # sidecar Node/Electron castlabs do engine DRM (v0.2)
├── packaging/
│   ├── sitrus.spec                   # RPM (Fedora)
│   ├── sitrus.desktop
│   ├── sitrus.appdata.xml
│   ├── AppImageBuilder.yml  (ou AppRun + appimage-builder)
│   └── sitrus.svg
├── docs/{brief.md,plan.md,design-system.md}
├── scripts/probe.sh                  # valida endpoints/token com os cookies do usuário
├── Cargo.toml (workspace), rust-toolchain.toml, deny.toml, LICENSE, README.md
```

Workspace multi-crate desde o início para que `sitrus-api` e `sitrus-core` sejam testáveis sem UI.

## M0 — Scaffold, design system e CI (fecha com CI verde)

- Workspace Rust, `rust-toolchain.toml` fixando a toolchain, `LICENSE` (MIT), README inicial.
- `sitrus-ui`: tokens (espaço, raio, tipo, cor), tema claro/escuro, Inter embutida (OFL),
  `Button`, `ListRow`, `RailItem`, `FocusRing`.
- Janela eframe com rail de navegação + conteúdo + player bar (vazios) e `sitrus --gallery`
  renderizando todos os componentes em todos os estados, nos dois temas.
- `ci.yml`: fmt, clippy `-D warnings`, test, build release, actionlint, cargo-deny; cache `Swatinem/rust-cache`.
- Branch protection em `main` exigindo CI; Dependabot para cargo e actions.
- Aceite: PR de teste fica vermelho ao introduzir warning de clippy e verde ao corrigir;
  `--gallery` abre e mostra os componentes nos dois temas.

## M1 — Login, token e catálogo (read-only)

- `sitrus-api`: cliente `amp-api.music.apple.com` com headers do web player (`Origin`,
  `User-Agent`, `media-user-token`, bearer), refresh do token raspando o bundle JS.
- Login: janela wry/WebKitGTK apontando para `music.apple.com` — único ponto com web engine —,
  captura de cookies no fim do fluxo, persistência em `~/.local/share/sitrus/` (arquivo 0600).
- UI: `HeroHeader` + `ShelfRow` na home, biblioteca (álbuns/artistas/músicas) e playlists em
  `AlbumCard`/`ListRow` (modo lista), com estados vazio/erro/carregando
  (`EmptyState`, `ErrorBanner`, `SkeletonRow`).
- Testes: fixtures JSON dos endpoints; teste de erro 401 → tela de login.
- Aceite: com conta real, lista biblioteca e playlists; sem cookies, mostra login.

## M2 — Reprodução e player

- Spike curto (1 dia): GStreamer `playbin` vs rodio+symphonia+leitor HLS. Critério de escolha:
  toca as URLs de stream reais, seek funciona, e o custo de empacotamento no RPM/AppImage é aceitável.
- `sitrus-player`: play/pause/seek/volume, eventos para a UI, fila em `sitrus-core` (próxima,
  anterior, reordenação, repetição desligada no MVP).
- UI: `PlayerBar`, `SeekSlider`, `VolumeSlider`, `QueuePanel`, `ContextMenu` na faixa.
- Aceite: tocar playlist inteira, seek sem travar a UI (thread de áudio separada), fila correta.

## M3 — Empacotamento e release

- `packaging/sitrus.spec` com `Requires` corretos (inclui `webkit2gtk4.1`); build em container `fedora:latest`.
- AppImage em runner Ubuntu 22.04 (glibc antigo) via appimage-builder; decidir com número medido
  se WebKitGTK entra no AppImage ou fica como dependência do sistema.
- `release.yml`: tag `v*` → build release, job appimage, job rpm, `SHA256SUMS`, upload com
  `softprops/action-gh-release` e notas geradas; artefatos versionados por `Cargo.toml`.
- Teste de fumaça do pacote: instalar o RPM em container Fedora e rodar `sitrus --version`.
- Ícone, `.desktop` e capturas de tela no README.
- Aceite: `git tag v0.1.0 && git push --tags` produz o release completo sem passo manual.

## M4 — Fechamento

- README com instalação (AppImage/RPM), aviso de que é cliente não oficial e do risco de ToS.
- CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, templates de issue/PR.
- CHANGELOG gerado (release-plz ou git-cliff).

## Riscos conhecidos

- Endpoints não oficiais mudam sem aviso: manter `sitrus-api` pequeno e com fixtures atualizáveis.
- Distribuição legal: app não-oficial não pode usar marca, fontes SF nem ícones SF da Apple;
  nome, arte e ícones próprios, Inter (OFL) + Lucide (ISC).
- Áudio HLS/AAC é a maior incerteza técnica; por isso o spike vem antes da UI de player.
