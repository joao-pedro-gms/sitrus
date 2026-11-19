# Spike EME / WebKitGTK

**Status: bloqueado antes da observação no WebKitGTK.** Este arquivo ainda não contém veredito `EME_OK` ou `EME_UNSUPPORTED`: nenhum probe renderizado rodou no CI e não há saída de EME/Apple Music observada. Não inferir suporte pelo changelog, pelas strings da biblioteca nem pelo CDM instalado.

## Evidência local observada

`cargo build -p eme-probe` terminou com exit 0:

```text
Compiling eme-probe v0.1.0 (/home/joaopedrogms/Documentos/Projetos/Sitrus/spike/eme-probe)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```

`cargo build --locked -p eme-probe --features webview` terminou com exit 101 antes de compilar o probe. A máquina não encontra também dependências de desenvolvimento GLib/GTK, além de WebKitGTK/D-Bus:

```text
error: failed to run custom build command for `gobject-sys v0.18.0`
> PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 pkg-config --libs --cflags gobject-2.0 'gobject-2.0 >= 2.70'
Package gobject-2.0 was not found in the pkg-config search path.
The system library `gobject-2.0` required by crate `gobject-sys` was not found.
```

A saída também reportou `atk`, `gio-2.0`, `glib-2.0`, `gdk-3.0`, `gdk-pixbuf-2.0`, `cairo` e `pango` ausentes. Portanto, a expectativa de falhar somente em `libdbus-sys` não se confirmou neste host; não há sudo e nada foi instalado.

## CI ainda não executado

O workflow novo foi validado localmente com `actionlint -shellcheck= .github/workflows/spike-eme.yml` (exit 0), mas `gh workflow list` não o encontra no default branch. O dispatch solicitado retornou literalmente:

```text
HTTP 404: workflow spike-eme.yml not found on the default branch (https://api.github.com/repos/joao-pedro-gms/sitrus/actions/workflows/spike-eme.yml)
```

Assim, não existe run ID, log/artifact nem checagem de token do artifact. Para continuar, o workflow e o probe precisam primeiro estar publicados no default branch; depois disparar `gh workflow run spike-eme.yml`, aguardar `gh run watch ID --exit-status`, verificar o artifact e substituir este status pela saída literal do probe e seu veredito observado.

## Implicação para o engine (pendente do probe)

Sem resultado de `requestMediaKeySystemAccess('com.widevine.alpha')` renderizado no WebKitGTK, não há conclusão sobre a necessidade do sidecar. Se o probe observado retornar `EME_UNSUPPORTED`, a lane é sidecar Electron castlabs, com a versão de referência do CDM pinada em `4.10.3112.0` (Sidra `1.3.2` nesta máquina). O CDM existente é somente referência local e não deve ser redistribuído. Manter a postura de wrapper com CDM; não extrair chaves de conteúdo. `EME_OK` provaria apenas a negociação de acesso EME/Widevine, não reprodução licenciada ponta a ponta.
