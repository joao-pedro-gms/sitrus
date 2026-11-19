#!/usr/bin/env bash
# scripts/probe.sh — testa os endpoints do player do Sitrus contra a Apple,
# usando os cookies da sessão do usuário.
#
# Uso:
#   SITRUS_COOKIES='media-user-token=***; music-user-token=***; ...' \
#     [SITRUS_STOREFRONT=br] [SITRUS_SONG_ID=1450695739] \
#     scripts/probe.sh
#
# Os cookies são os do navegador em music.apple.com (só o par nome=valor, sem
# atributos). `media-user-token` é o que autoriza dados de biblioteca/me; o
# restante dos cookies autentica as chamadas em music.apple.com/api/...
#
# Segurança: o script NUNCA imprime a string de cookies nem nenhum token.
# Cada corpo de resposta vai para um arquivo temporário (0600, removido no
# fim) e só status HTTP, tipo de conteúdo, tamanho e tempo são impressos.
#
# Saída: uma linha [OK]/[ERR]/[SKIP] por endpoint; exit 0 se tudo respondeu
# sem 401/403 (auth viva), exit 1 se algum endpoint autenticado deu 401/403,
# exit 2 se faltar input (cookies).

set -euo pipefail
umask 077

SITRUS_STOREFRONT="${SITRUS_STOREFRONT:-br}"
SITRUS_SONG_ID="${SITRUS_SONG_ID:-1450695739}"  # "bad guy" (Billie Eilish) na loja br
SITRUS_COOKIES="${SITRUS_COOKIES:-}"

WEB_BASE="https://music.apple.com"
AMP_BASE="https://amp-api.music.apple.com"

UA="sitrus-probe/0.1 (Linux x86_64)"
CURL_OPTS=(-sS -L --max-time 20)

if [[ -z "${SITRUS_COOKIES}" ]]; then
  echo "ERRO: \$SITRUS_COOKIES vazio." >&2
  echo "Copie os cookies de music.apple.com (par nome=valor) e exporte:" >&2
  echo "  SITRUS_COOKIES='media-user-token=***; music-user-token=***' ${0}" >&2
  exit 2
fi

# media-user-token pode vir dentro da string de cookies ou como env separado.
MEDIA_USER_TOKEN="${SITRUS_MEDIA_USER_TOKEN:-}"
if [[ -z "${MEDIA_USER_TOKEN}" ]]; then
  MEDIA_USER_TOKEN="$(
    printf '%s' "${SITRUS_COOKIES}" |
      tr ';' '\n' |
      sed -n 's/^[[:space:]]*media-user-token=[[:space:]]*//p' |
      head -n 1
  )"
fi

TMP_BODY="$(mktemp)"
TMP_BUNDLE="$(mktemp)"
DEFERRED_STATUS=0

fail_auth() {
  DEFERRED_STATUS=1
  echo "[ERR] $1"
}

cleanup() {
  rm -f "${TMP_BODY}" "${TMP_BUNDLE}"
}
trap cleanup EXIT

# Curl genérico: nunca imprime headers; resultado em TMP_BODY.
probe() {
  local label="$1" method="$2" url="$3"
  shift 3
  local want_media_token="${1:-no}"
  shift || true

  if [[ "${want_media_token}" == "media-token" && -z "${MEDIA_USER_TOKEN}" ]]; then
    echo "[SKIP] ${label} — sem media-user-token nos cookies"
    return 0
  fi

  local extra_headers=()
  if [[ "${want_media_token}" == "media-token" ]]; then
    extra_headers+=(-H "media-user-token: ${MEDIA_USER_TOKEN}")
  fi

  # Delimitador "|" evita que campos vazios (ex.: content_type de um 301)
  # desloquem o parsing do read.
  local status ct elapsed
  status="$(curl "${CURL_OPTS[@]}" \
    -X "${method}" \
    -o "${TMP_BODY}" \
    -A "${UA}" \
    -H "Origin: ${WEB_BASE}" \
    -H "Referer: ${WEB_BASE}/${SITRUS_STOREFRONT}/" \
    -H "Cookie: ${SITRUS_COOKIES}" \
    "${extra_headers[@]}" \
    "$@" \
    -w '%{http_code}|%{content_type}|%{size_download}|%{time_total}' \
    "${url}" || printf '000|rede|-1|-1')"

  IFS='|' read -r status ct size elapsed <<< "${status}"

  if [[ "${status}" == "000" ]]; then
    fail_auth "${label} — sem resposta (rede/DNS?)"
    return 0
  fi

  [[ -z "${ct}" || "${ct}" == "-" ]] && ct="—"
  echo "[OK] ${label} — HTTP ${status} · ${ct} · ${size} bytes · ${elapsed}s"

  if [[ "${status}" == "401" || "${status}" == "403" ]]; then
    DEFERRED_STATUS=1
    echo "      ^ 401/403: sessão/cookies inválidos ou expirados para este endpoint"
  fi
  return 0
}

echo "== Sitrus probe (${SITRUS_STOREFRONT}) — nada sensível é impresso =="

# 1) Sanity pública (sem cookies no efeito prático — endpoint do iTunes).
probe "iTunes lookup (público)" "GET" \
  "https://itunes.apple.com/lookup?id=${SITRUS_SONG_ID}&country=${SITRUS_STOREFRONT}" "no"

# 2) Homepage do Music — valida que a sessão web está viva (o HTML também é
#    usado depois para achar o bundle com o developer token). Nada é impresso.
probe "Sessão web (homepage)" "GET" "${WEB_BASE}/${SITRUS_STOREFRONT}/?l=pt-BR" "no"

# Developer token do web player: não fica no HTML; está no bundle JS
# (ex.: /assets/index~*.js) referenciado pela homepage. Buscamos o bundle e
# extraímos a primeira string em formato JWT (eyJ...). O token fica só no
# arquivo temporário e NUNCA é impresso.
DEV_TOKEN=""
BUNDLE_PATH="$(sed -n 's/.*<script[^>]*src="\([^"]*\.js\)".*/\1/p' "${TMP_BODY}" | head -n 1 || true)"
BUNDLE_URL="${WEB_BASE}/${BUNDLE_PATH}"
case "${BUNDLE_PATH}" in
  /*) BUNDLE_URL="${WEB_BASE}${BUNDLE_PATH}" ;;
esac
# BUNDLE_PATH vazio ou URL absoluta:
[[ -z "${BUNDLE_PATH}" ]] && BUNDLE_URL=""
[[ "${BUNDLE_PATH}" == http* ]] && BUNDLE_URL="${BUNDLE_PATH}"

if [[ -n "${BUNDLE_URL}" ]]; then
  curl "${CURL_OPTS[@]}" -m 40 -o "${TMP_BUNDLE}" "${BUNDLE_URL}" || true
  DEV_TOKEN="$(grep -oE 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+' "${TMP_BUNDLE}" | head -n 1 || true)"
fi
# Fallback: alguns formatos embutem "jwt":"..." direto no HTML.
if [[ -z "${DEV_TOKEN}" ]]; then
  DEV_TOKEN="$(grep -oE '"jwt":"[^"]+"' "${TMP_BODY}" | head -n 1 | cut -d'"' -f4 || true)"
fi

AMP_AUTH=()
if [[ -n "${DEV_TOKEN}" ]]; then
  AMP_AUTH+=(-H "Authorization: Bearer ${DEV_TOKEN}")
else
  echo "[SKIP] Endpoints amp-api — developer token não encontrado (bundle/HTML mudou?)"
fi

if [[ -n "${DEV_TOKEN}" ]]; then
  # 3) Catálogo do app (auth: developer token).
  probe "amp-api: catálogo (song)" "GET" \
    "${AMP_BASE}/v1/catalog/${SITRUS_STOREFRONT}/songs/${SITRUS_SONG_ID}" \
    "no" "${AMP_AUTH[@]}"

  # 4) Biblioteca do usuário (auth: developer token + media-user-token).
  probe "amp-api: biblioteca (me/library/playlists)" "GET" \
    "${AMP_BASE}/v1/me/library/playlists?limit=2" \
    "media-token" "${AMP_AUTH[@]}"
fi

# 5) Endpoint do player (assets de reprodução) — usa os cookies da sessão.
#    Formato de resposta/erros pode mudar; só reportamos o status.
probe "player: song/play (cookies)" "POST" \
  "${WEB_BASE}/api/v1/catalog/${SITRUS_STOREFRONT}/songs/${SITRUS_SONG_ID}/play" \
  "no" -H 'Content-Type: application/json' -d '{}'

echo "== fim =="
if [[ "${DEFERRED_STATUS}" -ne 0 ]]; then
  echo "Resultado: 401/403 em endpoint autenticado — sessão expirada? Reexporte \$SITRUS_COOKIES." >&2
  exit 1
fi
echo "Resultado: endpoints responderam; auth parece viva."
exit 0
