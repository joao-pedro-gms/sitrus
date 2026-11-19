# Sitrus — arquitetura

Visão dos crates do workspace e do fluxo principal: login → api → core → player → ui.
As decisões de produto estão em [brief.md](brief.md), o plano em [plan.md](plan.md) e
o design em [design-system.md](design-system.md).

## Diagrama de crates

```
                    ┌──────────────┐
                    │  sitrus-app  │  binário eframe: janela, atalhos, telas,
                    │    (bin)     │  --gallery
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌───────────┐
        │sitrus-ui │ │sitrus-   │ │sitrus-    │
        │ (tema,   │ │core      │ │player     │
        │ tokens,  │ │(domínio: │ │(backend   │
        │ compon-  │ │bibliote- │ │de áudio:  │
        │ entes)   │ │ca, fila, │ │play/pause,│
        └──────────┘ │estado do │ │seek, vol.)│
                     │player)   │ └───────────┘
                     └────┬─────┘
                          │
                          ▼
                    ┌───────────┐        ┌──────────────┐
                    │sitrus-api │ ────── │ amp-api.     │
                    │(cliente   │ HTTPS  │ music.apple. │
                    │HTTP não   │        │ com (endpoin-│
                    │oficial +  │        │ ts do web    │
                    │provedor   │        │ player)      │
                    │de token/  │        └──────────────┘
                    │cookies)   │
                    └─────┬─────┘
                          │ captura de cookies (único uso de web engine)
                          ▼
                    ┌───────────┐
                    │ webview   │  wry/WebKitGTK, só para o login,
                    │ (login)   │  atrás da feature `webview`
                    └───────────┘

   ┌──────────────┐   JSON-RPC 2.0 sobre WebSocket (127.0.0.1,
   │sitrus-engine │   porta efêmera, token por execução)
   │ (trait       │ ──────────────────────────────┐
   │  Engine,     │                               ▼
   │  erros       │                        ┌──────────────┐   spawn    ┌──────────────┐
   │  tipados com │                        │ engine/      │ ─────────▶ │ Electron     │
   │  política de │                        │ sidecar Node │            │ castlabs     │
   │  fallback)   │                        │ (sidecar.js) │            │ 45.2.1+wvcus │
   └──────────────┘                        └──────────────┘            │ + CDM        │
        │  sem dependência de sitrus-app; erros com                    │ Widevine     │
        │  `requires_native_fallback() == true` mandam o               │ 4.10.3112.0  │
        │  consumidor cair no sitrus-player nativo                     └──────────────┘
```

Dependências apontam sempre para baixo: `sitrus-app` amarra tudo; `sitrus-core` e
`sitrus-api` **não conhecem a UI** e são testáveis sozinhos com fixtures JSON.
`sitrus-ui` é pura apresentação (tokens, tema, componentes) e não conhece API nem
player.

## Fluxo login → api → core → player → ui

1. **Login** (`webview`, feature `webview`): janela wry/WebKitGTK abre
   `music.apple.com`; o usuário autentica na página da Apple. No fim do fluxo, os
   cookies de sessão e o `media-user-token` são capturados e persistidos em
   `~/.local/share/sitrus/` com permissão `0600`. Este é o único ponto do app com
   web engine — o build padrão, sem a feature, não compila webview nenhuma.
2. **API** (`sitrus-api`): lê cookies/token do disco e monta o cliente HTTP para
   `amp-api.music.apple.com` com os headers do web player (`Origin`, `User-Agent`,
   `media-user-token`, bearer). Um `401` derruba a sessão e manda o usuário de volta
   à tela de login, sem panic.
3. **Core** (`sitrus-core`): transforma as respostas da API em tipos de domínio
   (artista, álbum, música, playlist) e mantém o estado da aplicação: biblioteca,
   playlists e a fila de reprodução (próxima, anterior, reordenação).
4. **Player** (`sitrus-player`): recebe do core a faixa atual e a URL de stream;
   executa play/pause/seek/volume numa thread de áudio separada e emite eventos
   (posição, fim de faixa, erro) de volta. A UI nunca bloqueia no áudio.
5. **UI** (`sitrus-app` + `sitrus-ui`): renderiza biblioteca, playlists, player bar
   e fila a partir do estado do core, e envia intenções de volta (tocar playlist,
   reordenar fila, seek). Estados vazio/erro/carregando fazem parte de cada tela.

## Por que assim

- Os endpoints não oficiais mudam sem aviso: `sitrus-api` é pequeno, isolado e
  coberto por fixtures, então uma mudança da Apple se concentra num crate só.
- `sitrus-core` e `sitrus-api` sem dependência de UI permitem testar regras de fila
  e parsing de respostas sem abrir janela.
- A webview fica atrás de feature porque WebKitGTK é a dependência mais pesada do
  projeto: quem só desenvolve UI/lógica não precisa dela para compilar.
- O engine DRM fica fora do processo do app (sidecar Node/Electron) porque EME não
  existe no WebKitGTK disponível ([spike-eme.md](spike-eme.md), veredito
  `EME_UNSUPPORTED`) e porque isso isola o CDM Widevine — baixado em runtime, nunca
  redistribuído. Detalhes do protocolo e do que já foi provado estão em
  [engine/README.md](../engine/README.md).
