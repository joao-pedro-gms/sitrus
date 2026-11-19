# Spike EME / WebKitGTK

**Status: veredito observado no CI — `EME_UNSUPPORTED`.** O probe renderizado rodou no WebKitGTK do CI e a API EME não existe na página: nem Widevine, nem ClearKey (`API_UNAVAILABLE` nos dois). A reprodução de faixa completa por engine baseada em wry/WebKitGTK está descartada no lado EME; a lane de reprodução DRM será sidecar Electron castlabs com CDM Widevine pinado (seção "Implicação para o node engine").

## Run observado

- Run: [`44568466443`](https://github.com/joaopedrogms/sitrus/actions/runs/44568466443), `Spike EME / WebKitGTK`, SHA `1b92d36`, workflow_dispatch em `main`.
- Verificação: `gh run view 44568466443 --json conclusion,status` → `{"conclusion":"success","status":"completed"}`.
- Artifact `eme-probe-log`: `xvfb-run -a cargo run --locked -p eme-probe --features webview`, Ubuntu 22.04, WebKitGTK 4.1 instalado por apt, wry 0.57.3 + gtk 0.18.2; build terminou `Finished \`dev\` profile ... in 50.49s`.

## Saída literal do probe (artifact `eme-probe-log/eme-probe.log`, linhas finais)

```text
(eme-probe:3943): dbind-WARNING **: 16:40:30.115: AT-SPI: Error retrieving accessibility bus address: org.freedesktop.DBus.Error.ServiceUnknown: The name org.a11y.Bus was not provided by any .service files
EME_PROBE keySystem=com.widevine.alpha result=API_UNAVAILABLE
EME_PROBE keySystem=org.w3.clearkey result=API_UNAVAILABLE
EME_PROBE verdict=EME_UNSUPPORTED widevine=API_UNAVAILABLE clearkey=API_UNAVAILABLE
APPLE_PAGE status=navigating url=https://music.apple.com
APPLE_PAGE status=loaded
APPLE_DRM_CONSOLE summary=none observed_for_s=20
EME_PROBE_COMPLETE
```

## O que o console reportou em music.apple.com

A página navegou, carregou (`APPLE_PAGE status=loaded`) e ficou observada por 20 s com hook em `console.error/console.warn/window.error/unhandledrejection` filtrando termos `widevine|drm|eme|encrypted media|key system|keysystem|media key`. O console reportou **nada** relacionado a DRM: `APPLE_DRM_CONSOLE summary=none observed_for_s=20`. Ou seja: a página carrega em WebKitGTK e não reclama de DRM, mas o veredito vem da ausência da própria superfície EME, não de erro de playback.

## Veredito: EME_UNSUPPORTED

- `result=API_UNAVAILABLE` significa que `navigator.requestMediaKeySystemAccess` não existe no WebKitGTK do CI — o probe não chegou a negociar nenhum key system (o `NotSupported` do EME não apareceu; a API inteira está ausente).
- Isso vale também para ClearKey, que não depende de CDM proprietário: o que falta é a superfície EME do WebKitGTK, não licenciamento Widevine.
- Escopo da evidência: observado no WebKitGTK 4.1 do Ubuntu 22.04 (CI). Verificação local na estação Fedora 45 não é possível (`cargo build --locked -p eme-probe --features webview` falha antes da compilação por ausência das libs de desenvolvimento GLib/GTK sem sudo — ver evidência local abaixo). Não inferir suporte por changelog, strings de biblioteca ou CDM instalado.

## Implicação para o node engine

`EME_UNSUPPORTED` ⇒ **o engine não vai a wry/WebKitGTK com EME**; segue a lane de sidecar:

1. Engine de reprodução com DRM: **sidecar castlabs Electron com CDM Widevine pinado em `4.10.3112.0`**, como o Sidra faz nesta máquina. O node engine deve integrar por esse caminho, sem tentar EME no webview embutido.
2. O CDM é somente referência local; não redistribuir. Manter postura de wrapper com CDM próprio; não extrair chaves de conteúdo.
3. Para telas de artista/álbum/playlist e demais UI web, o webview (wry/WebKitGTK) continua válido — a página carrega sem erro (`status=loaded`, console limpo). A restrição é apenas reprodução DRM.

## Correção aplicada durante a observação

A primeira execução (run `44567921390`, SHA `ab45233`) falhou em `spike/eme-probe/src/main.rs` com `error[E0432]: unresolved import 'gtk::Inhibit'` (`no 'Inhibit' in the root`) — o tipo `Inhibit` não existe no gtk/glib 0.18. Corrigido em `1b92d36`: `use gtk::{Inhibit, prelude::*};` → `use gtk::glib::Propagation;` + `Propagation::Proceed` no `connect_delete_event`. Nenhuma outra alteração de escopo (crates/ e Cargo.toml intactos).

## Evidência local (anterior à observação)

`cargo build -p eme-probe` (sem feature webview) terminou exit 0; `cargo build --locked -p eme-probe --features webview` falha com exit 101 antes de compilar o probe — a máquina não tem dependências de desenvolvimento GLib/GTK/WebKitGTK/D-Bus e não há sudo para instalá-las:

```text
error: failed to run custom build command for `gobject-sys v0.18.0`
> PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 pkg-config --libs --cflags gobject-2.0 'gobject-2.0 >= 2.70'
Package gobject-2.0 was not found in the pkg-config search path.
The system library `gobject-2.0` required by crate `gobject-sys` was not found.
```

A saída também reporta `atk`, `gio-2.0`, `glib-2.0`, `gdk-3.0`, `gdk-pixbuf-2.0`, `cairo` e `pango` ausentes. Por isso o veredito foi obtido via CI.
