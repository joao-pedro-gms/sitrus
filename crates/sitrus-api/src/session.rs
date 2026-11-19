//! Sessão do usuário no player web da Apple Music.
//!
//! Guarda `media_user_token`, cookies e loja (storefront). Persiste em
//! `~/.local/share/sitrus/session.json` — caminho derivado com o crate
//! `directories` — sempre com permissão 0600. Para o caminho manual, sem
//! webview, [`Session::from_pasted_cookies`] aceita o header `Cookie` colado à
//! mão (copiado das devtools do navegador).

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nome do diretório do aplicativo dentro de `~/.local/share`.
const APP_DIR_NAME: &str = "sitrus";
/// Nome do arquivo de sessão.
const SESSION_FILE_NAME: &str = "session.json";
/// Permissões do arquivo de sessão: leitura/escrita só para o dono.
const FILE_MODE: u32 = 0o600;
/// Permissões do diretório do aplicativo quando precisamos criá-lo.
const DIR_MODE: u32 = 0o700;

/// Sessão autenticada do usuário no player web.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Session {
    /// Media User Token do player web (cabeçalho `Media-User-Token`).
    #[serde(default)]
    pub media_user_token: Option<String>,
    /// Loja alvo em minúsculas (`us`, `br`, …); vazia quando desconhecida.
    #[serde(default)]
    pub storefront: String,
    /// Cookies capturados do player web, por nome.
    #[serde(default)]
    pub cookies: BTreeMap<String, String>,
}

impl Session {
    /// Constrói a sessão a partir de cookies colados à mão (caminho manual sem
    /// webview): aceita o header `Cookie` copiado das devtools ou pares
    /// `nome=valor` separados por `;` ou por linha.
    ///
    /// Reconhece o media-user-token sob variações de nome
    /// (`MEDIA_USER_TOKEN`, `media-user-token`, `mediaUserToken`…) e deriva a
    /// loja do cookie `geo` quando presente. Pedaços sem `nome=valor` são
    /// ignorados silenciosamente.
    #[must_use]
    pub fn from_pasted_cookies(pasted: &str) -> Self {
        let mut cookies = BTreeMap::new();
        for raw_chunk in pasted.split([';', '\n', '\r']) {
            let chunk = strip_cookie_prefix(raw_chunk.trim());
            let Some((name, value)) = chunk.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            cookies.insert(name.to_owned(), value.trim().to_owned());
        }
        let media_user_token = cookies
            .iter()
            .find(|(name, _)| is_media_user_token(name))
            .map(|(_, value)| value.clone());
        let storefront = cookies
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("geo"))
            .map_or(String::new(), |(_, value)| value.to_lowercase());
        Self {
            media_user_token,
            storefront,
            cookies,
        }
    }

    /// Valor do header `Cookie` pronto para envio (`nome=valor; ...`).
    /// [`None`] quando a sessão não tem cookies.
    #[must_use]
    pub fn cookie_header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        let joined = self
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        Some(joined)
    }

    /// Caminho do arquivo de sessão:
    /// `~/.local/share/sitrus/session.json`, derivado com
    /// `directories::BaseDirs`.
    ///
    /// # Erros
    /// Falha quando não há diretório base do usuário (`$HOME` ausente).
    pub fn session_path() -> io::Result<PathBuf> {
        let dirs = directories::BaseDirs::new().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "diretório base do usuário ($HOME) indisponível",
            )
        })?;
        Ok(dirs.data_dir().join(APP_DIR_NAME).join(SESSION_FILE_NAME))
    }

    /// Grava a sessão com permissão 0600 e devolve o caminho escrito.
    ///
    /// # Erros
    /// Encaminha falhas de I/O (diretório, permissões, escrita).
    pub fn save(&self) -> io::Result<PathBuf> {
        let path = Self::session_path()?;
        self.save_to(&path)?;
        Ok(path)
    }

    /// Carrega a sessão salva; `Ok(None)` quando o arquivo ainda não existe
    /// (primeira execução).
    ///
    /// # Erros
    /// `InvalidData` quando o JSON está corrompido; demais erros de I/O
    /// seguem como estão.
    pub fn load() -> io::Result<Option<Self>> {
        let path = Self::session_path()?;
        Self::load_from(&path)
    }

    /// Versão redirecionável de [`Session::save`] — usada nos testes e por
    /// usuários avançados que queiram guardar a sessão em outro lugar.
    ///
    /// # Erros
    /// Encaminha falhas de I/O (diretório, permissões, escrita).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            tighten_dir_permissions(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("falha ao serializar sessão: {e}"),
            )
        })?;
        write_secure(path, json.as_bytes())
    }

    /// Versão redirecionável de [`Session::load`].
    pub fn load_from(path: &Path) -> io::Result<Option<Self>> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let session = serde_json::from_str(&text).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("session.json corrompido: {e}"),
                    )
                })?;
                Ok(Some(session))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// Escreve `bytes` garantindo modo 0600 no Unix: aperta permissões de arquivo
/// preexistente antes de sobrescrever e cria novos já restritivos.
fn write_secure(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if path.exists() {
            fs::set_permissions(path, fs::Permissions::from_mode(FILE_MODE))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = FILE_MODE;
        fs::write(path, bytes)?;
    }
    Ok(())
}

/// Reduz para 0700 as permissões do diretório que se chama `sitrus` (o diretório
/// do aplicativo). Para caminhos de outros nomes — testes, por exemplo — não
/// mexe em nada.
fn tighten_dir_permissions(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if dir.file_name().and_then(OsStr::to_str) == Some(APP_DIR_NAME) {
            let _ = fs::set_permissions(dir, fs::Permissions::from_mode(DIR_MODE));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (dir, DIR_MODE);
    }
}

/// Remove o prefixo `Cookie:` (qualquer caixa) de um pedaço colado.
fn strip_cookie_prefix(chunk: &str) -> &str {
    const PREFIX: &[u8] = b"Cookie:";
    let bytes = chunk.as_bytes();
    if bytes.len() >= PREFIX.len() && bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return chunk[PREFIX.len()..].trim_start();
    }
    chunk
}

/// Reconhece o media user token sob variações de nome/caixa.
fn is_media_user_token(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    normalized == "mediausertoken"
}
