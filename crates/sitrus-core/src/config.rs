//! Configuração persistente do Sitrus em TOML.

use std::path::{Path, PathBuf};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Nome do diretório da aplicação dentro do diretório XDG de configuração.
pub const CONFIG_DIRECTORY_NAME: &str = "sitrus";
/// Nome do arquivo de configuração.
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Alias legado para consumidores que usavam o nome anterior.
pub const SETTINGS_FILE_NAME: &str = CONFIG_FILE_NAME;
/// Quantidade acima da qual o modo lista automático é selecionado.
pub const DEFAULT_LIST_MODE_THRESHOLD: usize = 200;

/// Tema solicitado pela configuração; `System` acompanha o ambiente.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Segue o tema do sistema.
    #[default]
    System,
    /// Tema claro.
    Light,
    /// Tema escuro.
    Dark,
}

/// Preferências persistentes do aplicativo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Tema da interface.
    pub theme: Theme,
    /// Volume inicial, limitado ao intervalo `0.0..=1.0` ao ser consultado.
    pub volume: f32,
    /// Ativa a escolha automática do modo lista para bibliotecas grandes.
    pub automatic_list_mode: bool,
    /// Número de itens que habilita o modo lista quando automático está ativo.
    pub list_mode_threshold: usize,
    /// Fator opcional de escala da interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<f32>,
    /// Código de storefront usado pelas futuras integrações de catálogo.
    pub storefront: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            volume: 1.0,
            automatic_list_mode: true,
            list_mode_threshold: DEFAULT_LIST_MODE_THRESHOLD,
            ui_scale: None,
            storefront: default_storefront(),
        }
    }
}

/// Diretório de configuração por usuário, respeitando XDG.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().to_owned())
}

/// Caminho padrão `~/.config/sitrus/config.toml` (ou o diretório XDG equivalente).
#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|directory| directory.join(CONFIG_DIRECTORY_NAME).join(CONFIG_FILE_NAME))
}

/// Código de storefront padrão.
fn default_storefront() -> String {
    "us".to_owned()
}

impl Settings {
    /// Cria preferências com os valores padrão do projeto.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Informa se uma biblioteca deve usar o modo lista automático.
    #[must_use]
    pub fn should_use_list_mode(&self, item_count: usize) -> bool {
        self.automatic_list_mode && item_count > self.list_mode_threshold
    }

    /// Lê e interpreta um arquivo TOML em `path`.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let raw = std::fs::read_to_string(path)?;
        tracing::debug!(path = %path.display(), "configuração carregada");
        Ok(toml::from_str(&raw)?)
    }

    /// Grava a configuração em `path`, criando os diretórios pais.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string(self)?)?;
        Ok(())
    }

    /// Carrega o caminho padrão; arquivo ausente significa preferências padrão.
    pub fn load_default() -> Result<Self, CoreError> {
        let Some(path) = default_config_path() else {
            return Ok(Self::default());
        };
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Salva no caminho padrão do usuário.
    pub fn save_default(&self) -> Result<(), CoreError> {
        let path = default_config_path().ok_or(CoreError::ConfigDirectoryUnavailable)?;
        self.save(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_preserves_fields() {
        let path = std::env::temp_dir()
            .join("sitrus-core-tests-roundtrip")
            .join(CONFIG_FILE_NAME);
        let settings = Settings {
            theme: Theme::Dark,
            volume: 0.65,
            automatic_list_mode: false,
            list_mode_threshold: 250,
            ui_scale: Some(1.25),
            storefront: "br".to_owned(),
        };
        settings.save(&path).expect("gravação de configuração");
        let reloaded = Settings::load(&path).expect("leitura de configuração");
        std::fs::remove_file(&path).ok();
        assert_eq!(reloaded, settings);
    }

    #[test]
    fn configuration_path_uses_sitrus_config_toml() {
        if let Some(path) = default_config_path() {
            assert!(path.ends_with("sitrus/config.toml"));
        }
    }
}
