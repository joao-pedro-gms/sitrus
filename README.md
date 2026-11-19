# Sitrus

Cliente nativo de Apple Music para Linux, escrito em Rust com interface egui/eframe.
Sitrus entra na sua conta da Apple Music, mostra sua biblioteca e playlists, navega
por artistas e álbuns do catálogo e oferece um player com controles de reprodução —
tudo numa janela nativa, sem web engine na interface principal.

> [!WARNING]
> **Cliente não oficial.** Sitrus **não é afiliado, endossado ou patrocinado pela
> Apple Inc.** e não usa nenhuma API oficial (MusicKit). Ele se comunica com os mesmos
> endpoints do web player em `music.apple.com` usando os cookies da sua sessão. **O uso
> deste aplicativo pode violar os Termos de Serviço da Apple** e pode deixar de funcionar
> a qualquer momento se a Apple alterar o web player. A reprodução com DRM depende do
> CDM Widevine (tecnologia do Google, baixada em runtime e não redistribuída pelo
> projeto) e pode quebrar por mudanças fora do nosso controle. Use por sua conta e
> risco. "Apple Music" é marca registrada da Apple Inc.

## O que o Sitrus faz (v0.2.0)

- **Login embutido**: uma janela webview (WebKitGTK) aponta para `music.apple.com`;
  após a autenticação, cookies e `media-user-token` são capturados e guardados
  localmente em `~/.local/share/sitrus/` (arquivo com permissão `0600`).
- **Biblioteca e playlists**: listagem das músicas e playlists da sua conta, em
  prateleiras ou modo lista. Clicar numa playlist abre a página dela com as faixas.
- **Artista, álbum e playlist**: páginas de detalhe com capa, lista de faixas e
  botão de tocar. O artista tem abas "Álbuns" e "Músicas" — dá para ir da busca ao
  artista e dele ao álbum, voltando com `Escape`.
- **Busca**: no catálogo da Apple Music, com abas Artistas/Álbuns/Faixas; sem rede,
  cai no filtro local da sua biblioteca.
- **Player**: play/pause, próxima/anterior, seek e volume, com painel de fila
  **interativa** ("Tocando a seguir"): clique numa faixa para tocá-la, arraste
  para reordenar. Sobre **o que toca de fato**, veja a seção
  [Reprodução](#reprodução-o-que-toca-hoje) abaixo — ela descreve o comportamento
  real, sem promessas.

Fora do escopo por enquanto: letras, favoritos, download offline, rádio e
recomendações. Veja o [CHANGELOG](CHANGELOG.md) e o [plano de entrega](docs/plan.md).

## Reprodução: o que toca hoje

Há dois motores de reprodução no projeto, e é importante separar o que cada um faz
**hoje, no código**:

- **Player nativo (`sitrus-player`)** — backend de áudio local. Toca:
  - **prévias do catálogo** (as URLs `previewUrl` que a Apple expõe nas faixas de
    catálogo — busca, álbuns, artistas e playlists de curadoria);
  - **arquivos locais e streams HLS/AAC** passados diretamente ao backend.
- **Engine DRM (`crates/sitrus-engine` + sidecar `engine/`)** — camada de reprodução
  de faixa completa com DRM, **integrada ao app nesta versão**: faixas sem fonte
  nativa (por exemplo, as da sua biblioteca, que não expõem URL de reprodução)
  são roteadas ao engine quando ele está disponível
  (`crates/sitrus-app/src/player.rs`, `ActiveBackend::Engine`). O engine é um
  sidecar Node/Electron (build castlabs `45.2.1+wvcus`) que conversa com o crate
  Rust por JSON-RPC sobre WebSocket em `127.0.0.1` (porta efêmera, token por
  execução). O engine **não sobe no arranque**: o sidecar é criado na primeira
  faixa que exige DRM, em thread de fundo, com a interface já interativa, e a
  escolha do backend é por faixa, com política de fallback tipada
  (`EngineError::requires_native_fallback()`, veja abaixo).
  **Exige Node no sistema** e o dist do Electron é baixado no primeiro uso
  (~274 MB; nenhum binário Electron/CDM vai no pacote). **Limites honestos desta
  versão**: a **ponte de sessão está implementada** — os cookies e o
  `media-user-token` da sessão do app vão ao sidecar por stdin (nunca argv, env
  ou log) e são injetados no perfil do Electron antes do `loadURL`, o que está
  provado por teste com `cookies.get` e varredura de `/proc`; o que **não** está
  provado é o efeito disso na Apple: nenhuma página real autenticada foi aberta
  nem nenhuma faixa licenciada foi tocada nesta estação, e o licenciamento
  Widevine real (VMP/UAT/produção) **não foi exercitado**; o **adapter in-page**
  aciona o player web com contrato de eventos e erro tipado
  (`page-control-unavailable`), mas foi validado contra uma fachada mock do
  MusicKit, não contra a página real.

**Como validar o áudio com a sua conta:** o roteiro passo a passo (login manual,
qual faixa dispara o engine e o que cada sinal significa) está em
[docs/validar-audio.md](docs/validar-audio.md).

**Como o engine é escolhido** (contrato implementado em `crates/sitrus-engine`): o
consumidor tenta subir o `SidecarEngine`, que faz spawn de `node engine/sidecar.js`
com token gerado por execução. Os erros são tipados, e a política de decisão é
`EngineError::requires_native_fallback()`:

- **cai no player nativo** quando o sidecar está indisponível (Node ausente,
  `engine/` incompleto, processo morto), quando o handshake é rejeitado ou quando o
  protocolo/conexão quebra (`SidecarUnavailable`/`Handshake`/`Protocol`);
- **não cai no nativo** em falhas de reprodução reportadas pelo protocolo
  (`Rpc`) nem em entrada inválida — essas são exibidas como erro de reprodução.

## Dependência de Widevine (CDM)

O engine DRM usa o CDM Widevine. O caminho de EME no WebKitGTK da janela principal
foi descartado depois de um spike com evidência (`docs/spike-eme.md`: veredito
`EME_UNSUPPORTED` — nem ClearKey existe no WebKitGTK testado).

- **Instalação em runtime, nada redistribuído**: no primeiro uso, o Electron
  castlabs baixa e instala o CDM em `~/.config/sitrus-engine/WidevineCdm/`
  (versão `4.10.3112.0`, **21 MB** medidos nesta estação). Nenhum binário Electron
  ou CDM vai no repositório, no RPM ou no AppImage.
- **Versão pinada**: a versão do CDM acompanha o build do Electron pinado em
  `engine/package.json` (`@castlabs/electron-releases` na tag `v45.2.1+wvcus` → CDM
  `4.10.3112.0`).
- **Quando o CDM quebra**: se a instalação/atualização do componente falhar, o
  processo Electron registra o erro em stderr e emite um evento de erro
  (`widevine components failed: …`) que o cliente Rust recebe como erro de
  reprodução (`EngineEvent::Error`), sem derrubar o lado Rust.
- **Como recuperar ou trocar a versão**:
  - para forçar reinstalação do CDM, apague `~/.config/sitrus-engine/WidevineCdm/`
    e rode o engine de novo (o `components.whenReady()` baixa de novo);
  - para trocar a versão do CDM, troque a tag do `@castlabs/electron-releases` em
    `engine/package.json` para outro build `*+wvcus`, rode `npm ci` e
    `node node_modules/@castlabs/electron-releases/install.js` dentro de `engine/`
    (a versão do CDM é a do build Electron escolhido).

## Custo de instalação (medido)

Números medidos nesta estação (Fedora 45, 2026-11-19) e nos artefatos publicados —
não estimados. O ponto principal: **o Electron e o CDM não vão dentro dos pacotes**;
o RPM e o AppImage instalam apenas as fontes JS do sidecar (64 KB) e o bootstrap
(`npm ci` + instalador do build castlabs) baixa o resto com Node presente (no RPM,
`nodejs` é `Recommends`).

| Medição | Valor |
| --- | --- |
| RPM v0.1.0 publicado (sem engine) | 9.507.856 bytes (~9,1 MiB) |
| RPM 0.2.0 local com as fontes do engine (rpmbuild nesta estação) | 9.465.474 bytes (~9,0 MiB) |
| AppImage v0.1.0 publicado (sem engine) | 90.723.520 bytes (~86,5 MiB) |
| Payload do engine dentro do pacote (fontes JS de `engine/`, sem `node_modules`) | 64 KB |
| `engine/node_modules/` completo em runtime (`npm ci`) | 288 MB |
| — do qual o dist do Electron castlabs | 274 MB |
| CDM Widevine em `~/.config/sitrus-engine/WidevineCdm/` | 21 MB (`4.10.3112.0`) |
| Cache do instalador em `~/.cache/electron` (pode ser limpo) | 354 MB |

Ou seja: ter o engine **sidecar** custa ~64 KB dentro do pacote (a diferença entre
os RPMs acima fica dentro do ruído de build) e ~274 MB + 21 MB baixados em runtime,
fora do pacote. O AppImage 0.2.0 ainda não foi construído nesta estação; como a
recipe instala o mesmo conjunto de fontes de 64 KB, o delta de pacote é o mesmo —
o número final do AppImage com engine fica pendente de medição no release.

## Capturas de tela

> [!NOTE]
> **Pendentes.** As capturas de tela serão adicionadas em um release futuro.

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

O login embutido via webview usa WebKitGTK 4.1: no RPM ele é resolvido como
dependência (`webkit2gtk4.1`); no AppImage a biblioteca já vem embutida no pacote.

O engine DRM (sidecar Node/Electron) **não vai embutido no AppImage** e exige
**Node.js instalado no sistema**: sem Node, o app cai silenciosamente no player
nativo (prévias e arquivos locais). O dist do Electron (~274 MB) é baixado no
primeiro uso.

### RPM (Fedora)

```bash
# Baixe sitrus-<versão>-1.x86_64.rpm do release mais recente
sudo dnf install ./sitrus-*.rpm
sitrus   # ou abra pelo menu de aplicativos
```

O pacote RPM declara `webkit2gtk4.1` como dependência obrigatória e `nodejs` como
recomendada (necessário apenas para o engine DRM sidecar), então o `dnf` resolve
sozinho.

## Build local

Requisitos: toolchain Rust estável (ver `rust-toolchain.toml`) e cabeçalhos ALSA
(`alsa-lib-devel` no Fedora) para o backend de áudio.

```bash
# Interface egui pura, sem webview (o login embutido fica indisponível)
cargo build --release
```

### AppImage local (para testar nesta máquina)

```bash
cargo build --release
scripts/build-appimage-local.sh ~/Downloads/Sitrus-x86_64.AppImage
```

Essa AppImage usa as bibliotecas do sistema (glibc, GTK3, ALSA, X11/Wayland) e
não embute WebKitGTK: serve para rodar na distro onde foi construída, com login
manual (`--login-manual`). A AppImage **do release** (CI, ubuntu-22.04) é a
portátil, com WebKitGTK embutido. Em nenhuma das duas o engine DRM consegue fazer
o bootstrap do Electron: ele precisa de `engine/node_modules` num diretório
gravável, e o mount da AppImage é somente leitura — para validar áudio use o
binário de `target/release/` no checkout do repositório.

### Variante com webview (login embutido)

A janela de login usa WebKitGTK via `wry`. Para compilá-la, é preciso o pacote de
desenvolvimento **`webkit2gtk4.1-devel`** (Fedora):

```bash
sudo dnf install webkit2gtk4.1-devel
cargo build --release --features webview
```

Sem o `-devel`, o build com `--features webview` falha na ligação nativa; o build
padrão (sem a feature) continua funcionando.

### Engine DRM (opcional, sidecar)

O engine é um processo separado (Node + Electron castlabs) e não muda o build do
app em Rust:

```bash
cd engine
npm ci                                                    # deps (ws + pacote do Electron castlabs)
node node_modules/@castlabs/electron-releases/install.js  # baixa o dist (~274 MB)
node test/roundtrip.js                                    # prova de protocolo (modo mock)
```

Detalhes do protocolo, da segurança (token por execução, WS só em `127.0.0.1`) e do
que já foi provado estão em [engine/README.md](engine/README.md).

## Navegação e atalhos de teclado

Fluxos de navegação implementados:

- **Busca → artista**: aba "Artistas" dos resultados abre a página do artista.
- **Busca → álbum**: aba "Álbuns" abre a página do álbum.
- **Artista → álbum**: na aba "Álbuns" do artista, o card abre a página do álbum
  (busca → artista → álbum funciona numa pilha só).
- **Playlists**: a tela Playlists e a prateleira "Suas playlists" da Home abrem a
  página da playlist da sua biblioteca.
- **Voltar**: `Escape` volta sempre que há telas empilhadas (mesmo com campo de
  texto focado); dentro das páginas de artista/álbum/playlist, a seta `←` também
  volta (nessas telas ela não faz seek); o botão "Voltar" do cabeçalho faz o mesmo.

Atalhos (conferidos no código, `crates/sitrus-app/src/app.rs`):

| Tecla | Ação |
| --- | --- |
| `Espaço` | Reproduzir / pausar |
| `←` / `→` | Voltar / avançar 10 s (em páginas de detalhe, `←` volta para a tela anterior) |
| `↑` / `↓` | Aumentar / diminuir o volume em 5% |
| `Enter` | Buscar (com o campo de busca focado) |
| `Escape` | Voltar na pilha de telas |
| `Ctrl` + `F` | Ir para a busca e focar o campo |
| `Ctrl` + `Q` | Sair do aplicativo |

Os atalhos simples (espaço e setas) são ignorados enquanto um campo de texto está
focado, e funcionam com a janela do app focada; atalhos globais de mídia não fazem
parte desta versão.

## Documentação

- [Arquitetura dos crates](docs/architecture.md)
- [Brief do produto](docs/brief.md)
- [Plano de entrega](docs/plan.md)
- [Sistema de design](docs/design-system.md)
- [Spike de EME/WebKitGTK](docs/spike-eme.md)
- [Sidecar do engine DRM](engine/README.md)

## Contribuindo, segurança e conduta

- [CONTRIBUTING.md](CONTRIBUTING.md) — como rodar fmt, clippy, testes e o padrão de commits.
- [SECURITY.md](SECURITY.md) — como reportar vulnerabilidades e o que é tratado como segredo.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — Contributor Covenant 2.1.

## Licença

MIT. Veja [LICENSE](LICENSE).
