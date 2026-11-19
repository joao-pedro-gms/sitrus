# Validar áudio de faixa completa

Roteiro para o único ponto da v0.2 que nenhum teste nesta estação pode observar:
reproduzir uma faixa da sua conta, completa, pelo engine com DRM. Tudo aqui parte
de `/home/joaopedrogms/Documentos/Projetos/Sitrus`.

O que já está instalado nesta máquina: Node no `PATH`, `engine/node_modules`
(288 MB, com o Electron castlabs) e o CDM Widevine `4.10.3112.0` em
`~/.config/sitrus-engine/WidevineCdm/`.

## 1. Construir o binário

```bash
cargo build --release
./target/release/sitrus --version   # sitrus 0.2.0
```

## 2. Entrar com a sua sessão

O login por webview exige a feature `webview` (precisa de `webkit2gtk-4.1-devel`,
ausente aqui). O caminho que funciona nesta máquina é o login manual:

```bash
./target/release/sitrus --login-manual
```

No navegador, com o player do `music.apple.com` aberto e **tocando**:

1. DevTools (`F12`) → aba **Rede**;
2. filtre por `amp-api.music.apple.com`;
3. clique num pedido e copie o **cabeçalho `Cookie:` inteiro** (botão direito →
   *Copy value*);
4. cole no campo da janela do Sitrus e confirme.

O app exige que o cookie `media-user-token` esteja no texto — é ele que autoriza
a sua biblioteca. O cookie `geo` define a loja. A sessão vai para
`~/.local/share/sitrus/session.json` com permissão `0600`.

## 3. Tocar uma faixa que só o engine consegue tocar

Isto importa: faixas **de catálogo** (busca, álbum, artista) têm `previewUrl` e
tocam 30 s pelo player nativo. O engine só entra em faixa **sem fonte nativa** —
ou seja, música da **sua biblioteca**.

1. Abra a Home/biblioteca e clique numa faixa sua (ou numa playlist da sua conta).
2. O sidecar sobe neste momento, não no arranque: a primeira faixa DRM dispara o
   spawn em thread de fundo (`sitrus-engine-spawn`), o app segue responsivo.
3. A primeira execução pode baixar o dist do Electron (~274 MB) se
   `engine/node_modules` não estiver presente.

## 4. Ver o que a página faz (opcional, mas útil)

Por padrão a janela do Electron é oculta. Para inspecioná-la durante a
validação:

```bash
SITRUS_ENGINE_WINDOW=1 ./target/release/sitrus
```

A janela mostra `https://music.apple.com/` já autenticada pelos cookies
injetados antes do `loadURL`. Se ela abrir pedindo login, a ponte de sessão
falhou para a Apple e é isso que precisa ser reportado — não é problema de DRM.

## 5. O que observar

| Sinal | Onde aparece | Interpretação |
| --- | --- | --- |
| Áudio da faixa, `posição` avançando na barra | janela principal | critério `c1` cumprido |
| Toast de erro de reprodução | janela principal | erro RPC do engine (não cai no nativo de propósito) |
| Toast de aviso do engine | janela principal | sidecar indisponível → fallback nativo (prévia/arquivo) |
| Página pedindo login na janela do Electron | `SITRUS_ENGINE_WINDOW=1` | ponte de sessão não autenticou na Apple |
| Tela preta/`page-control-unavailable` | toast | página real não expôs `MusicKit` ao adapter |

Para ver o que o perfil do Electron observa sem depender do áudio, rode o app
com `SITRUS_ENGINE_WINDOW=1` e toque uma faixa da biblioteca: a janela do engine
mostra a página já autenticada (ou pedindo login, que é o sinal de falha da
ponte). Não existe ainda um comando de diagnóstico de sessão na CLI — o
`session_probe` do protocolo é interno e bloqueante, e chamá-lo exige um cliente
WebSocket próprio; se precisar dele, isso é trabalho novo, não passo de
validação.

## 6. Se não tocar, o que guardar para o relatório

Cole o resultado de cada item; eles separam as três falhas possíveis:

1. **Sessão** — a página do Electron abriu autenticada? (`SITRUS_ENGINE_WINDOW=1`)
2. **Controle** — houve toast `page-control-unavailable` ou "page load failed"?
3. **DRM** — houve erro de licença/CDM, ou o áudio simplesmente não veio?

Com esses três, a próxima correção tem alvo em vez de palpite.
