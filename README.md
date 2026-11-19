# Sitrus

Cliente nativo de Apple Music para Linux, escrito em Rust com interface egui/eframe.
Sitrus entra na sua conta da Apple Music, mostra sua biblioteca e playlists e reproduz
áudio com controles completos de player — tudo numa janela nativa, sem web engine na
interface principal.

> [!WARNING]
> **Cliente não oficial.** Sitrus **não é afiliado, endossado ou patrocinado pela
> Apple Inc.** e não usa nenhuma API oficial (MusicKit). Ele se comunica com os mesmos
> endpoints do web player em `music.apple.com` usando os cookies da sua sessão. **O uso
> deste aplicativo pode violar os Termos de Serviço da Apple** e pode deixar de funcionar
> a qualquer momento se a Apple alterar o web player. Use por sua conta e risco.
> "Apple Music" é marca registrada da Apple Inc.

## O que o Sitrus faz (v0.1.0)

- **Login embutido**: uma janela webview (WebKitGTK) aponta para `music.apple.com`;
  após a autenticação, cookies e `media-user-token` são capturados e guardados
  localmente em `~/.local/share/sitrus/` (arquivo com permissão `0600`).
- **Biblioteca**: listagem e navegação por artistas, álbuns e músicas da sua conta.
- **Playlists**: listar e abrir suas playlists e tocar a playlist inteira como fila.
- **Player**: play/pause, próxima/anterior, seek, volume e fila visível com reordenação.

Fora do escopo por enquanto: busca no catálogo global, letras, favoritos, download
offline, rádio e recomendações. Veja o [CHANGELOG](CHANGELOG.md) e o
[plano de entrega](docs/plan.md).

## Capturas de tela

> [!NOTE]
> **Pendentes.** As capturas de tela serão adicionadas quando a interface da v0.1.0
> estiver implementada.

## Instalação

Os binários são publicados na página de
[Releases](https://github.com/joao-pedro-gms/sitrus/releases) a cada tag `v*`,
junto com o arquivo `SHA256SUMS` para verificação.

### AppImage (qualquer distribuição x86_64)

```bash
# Baixe Sitrus-x86_64.AppImage e SHA256SUMS do release mais recente
sha256sum --check SHA256SUMS --ignore-missing

chmod +x Sitrus-x86_64.AppImage
./Sitrus-x86_64.AppImage
```

O login embutido via webview exige **WebKitGTK 4.1** instalado no sistema
(`webkit2gtk4.1` no Fedora). Sem ele, o app abre mas a janela de login não funciona.

### RPM (Fedora)

```bash
# Baixe sitrus-<versão>-1.x86_64.rpm do release mais recente
sudo dnf install ./sitrus-*.rpm
sitrus   # ou abra pelo menu de aplicativos
```

O pacote RPM declara `webkit2gtk4.1` como dependência, então o `dnf` resolve sozinho.

## Build local

Requisitos: toolchain Rust estável (ver `rust-toolchain.toml`) e cabeçalhos ALSA
(`alsa-lib-devel` no Fedora) para o backend de áudio.

```bash
# Interface egui pura, sem webview (o login embutido fica indisponível)
cargo build --release
```

### Variante com webview (login embutido)

A janela de login usa WebKitGTK via `wry`. Para compilá-la, é preciso o pacote de
desenvolvimento **`webkit2gtk4.1-devel`** (Fedora):

```bash
sudo dnf install webkit2gtk4.1-devel
cargo build --release --features webview
```

Sem o `-devel`, o build com `--features webview` falha na ligação nativa; o build
padrão (sem a feature) continua funcionando.

## Atalhos de teclado

| Tecla | Ação |
| --- | --- |
| `Espaço` | Reproduzir / pausar |
| `Ctrl` + `→` | Próxima faixa |
| `Ctrl` + `←` | Faixa anterior |
| `Shift` + `→` / `Shift` + `←` | Avançar / voltar 10 s |
| `↑` / `↓` | Aumentar / diminuir volume |
| `Ctrl` + `Q` | Mostrar / ocultar a fila |

Os atalhos funcionam com a janela do app focada; atalhos globais de mídia não fazem
parte da v0.1.0.

## Documentação

- [Arquitetura dos crates](docs/architecture.md)
- [Brief do produto](docs/brief.md)
- [Plano de entrega](docs/plan.md)
- [Sistema de design](docs/design-system.md)

## Contribuindo, segurança e conduta

- [CONTRIBUTING.md](CONTRIBUTING.md) — como rodar fmt, clippy, testes e o padrão de commits.
- [SECURITY.md](SECURITY.md) — como reportar vulnerabilidades e o que é tratado como segredo.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — Contributor Covenant 2.1.

## Licença

MIT. Veja [LICENSE](LICENSE).
