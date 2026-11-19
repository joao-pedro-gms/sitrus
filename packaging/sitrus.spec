# RPM do Sitrus.
#
# Este spec empacota o binário pré-compilado pela esteira de release
# (.github/workflows/release.yml, container fedora:latest) — o %build é
# intencionalmente um no-op: o artefato target/release/sitrus entra no
# Source0 já compilado e o %install o copia para %{_bindir}.
#
# Build local (a partir da raiz do repo, com o binário já em target/release):
#   VER=0.1.0
#   tar --exclude-vcs --exclude .github --transform "s,^\./,sitrus-${VER}/," \
#     -czf /tmp/sitrus-${VER}.tar.gz .
#   rpmbuild -bb --define "sitrus_version ${VER}" \
#     --define "_topdir ~/rpmbuild" --define "_sourcedir /tmp" packaging/sitrus.spec
#
# A versão é passada pela esteira via macro: rpmbuild -D "sitrus_version 0.1.0".

# Binário pré-compilado: sem fontes Rust no build tree do rpmbuild, o
# find-debuginfo geraria %files vazio (erro "Arquivo %files vazio
# .../debugsourcefiles.list"). Desativa o subpacote de debug.
%global debug_package %{nil}

Name:           sitrus
Version:        %{?sitrus_version}%{!?sitrus_version:0.0.0}
Release:        1
Summary:        Cliente nativo de Apple Music para Linux
License:        MIT
URL:            https://github.com/joao-pedro-gms/sitrus
Source0:        %{name}-%{version}.tar.gz
BuildArch:      x86_64

# Mínimo mesmo: o binário é pré-compilado (Rust, edition 2024) pela esteira e
# entra no Source0; nada do RPM precisa de toolchain no host do rpmbuild.
BuildRequires:  coreutils

# Link-time do binário: libasound.so.2 (áudio via ALSA) e
# libwebkit2gtk-4.1.so.0 (webview de login). O resto dos sonames é
# coberto pelas Requires automáticas do rpmbuild.
# Requires/recommends:
# - alsa-lib e webkit2gtk4.1: áudio nativo e webview de login (obrigatórios).
# - nodejs: recomendado — só o engine DRM (sidecar Electron castlabs,
#   crates/sitrus-engine + engine/) precisa dele em runtime; sem ele, o app
#   cai no sitrus-player nativo (ver engine/README.md). O dist do Electron
#   NÃO é redistribuído no RPM: é baixado no bootstrap (npm ci + instalador
#   do @castlabs/electron-releases, tag v45.2.1+wvcus, CDM Widevine 4.10.3112.0).
Requires:       alsa-lib
Requires:       webkit2gtk4.1
Recommends:     nodejs

%description
Cliente nativo (Rust + egui) para Apple Music no Linux: login embutido por
webview usando os cookies da sessão do usuário, navegação pela biblioteca e
playlists do usuário e player com play/pause, próxima/anterior, seek e
volume (fila interativa: clique toca a faixa, arraste reordena).

Projeto independente, não afiliado à Apple. Licença MIT.

%prep
%setup -q

%build
# O build aconteceu antes do rpmbuild (ver release.yml): o binário entra no
# Source0 já compilado. Falha alto se faltar.
test -x target/release/sitrus

%install
install -D -p -m 0755 target/release/sitrus        %{buildroot}%{_bindir}/%{name}
install -D -p -m 0644 packaging/sitrus.desktop     %{buildroot}%{_datadir}/applications/%{name}.desktop
install -D -p -m 0644 packaging/sitrus.svg         %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{name}.svg
install -D -p -m 0644 packaging/sitrus.appdata.xml %{buildroot}%{_metainfodir}/%{name}.appdata.xml
# Fontes do sidecar do engine DRM (JS): sem node_modules e sem binário
# Electron — nada de CDM/binário redistribuído (postura de wrapper, ver
# docs/spike-eme.md e engine/README.md). O bootstrap (npm ci + baixar o
# Electron castlabs) ocorre em empacotamento/primeiro uso, com Node presente.
install -d -p -m 0755 %{buildroot}%{_libexecdir}/%{name}/engine/lib \
                      %{buildroot}%{_libexecdir}/%{name}/engine/mock \
                      %{buildroot}%{_libexecdir}/%{name}/engine/test
install -p -m 0644 engine/package.json engine/package-lock.json \
                  engine/sidecar.js engine/main.js engine/preload.js \
                  %{buildroot}%{_libexecdir}/%{name}/engine/
install -p -m 0644 engine/lib/*.js %{buildroot}%{_libexecdir}/%{name}/engine/lib/
install -p -m 0644 engine/mock/*   %{buildroot}%{_libexecdir}/%{name}/engine/mock/
install -p -m 0644 engine/test/*.js %{buildroot}%{_libexecdir}/%{name}/engine/test/

%files
%{_bindir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{name}.svg
%{_metainfodir}/%{name}.appdata.xml
%{_libexecdir}/%{name}/engine/
%license LICENSE

%changelog
* Thu Nov 19 2026 Sitrus Release Automation <ci@sitrus.example.invalid> - 0.1.0-1
- Versão inicial: cliente nativo egui para Apple Music no Linux; login por
  webview WebKitGTK, biblioteca/playlists e player completo.
