//! Localização dos diretórios do aplicativo em conformidade com XDG.

use directories::ProjectDirs;
use std::path::PathBuf;

/// Qualificador reverso usado para derivar os caminhos XDG do projeto.
const PROJECT_QUALIFIER: &str = "dev";

/// Organização responsável pelo projeto (vazio: projeto individual).
const PROJECT_ORGANIZATION: &str = "";

/// Nome do aplicativo como será visto nos diretórios do usuário.
const PROJECT_NAME: &str = "Sitrus";

/// Retorna os diretórios padrão do projeto, quando o ambiente permite derivá-los.
#[must_use]
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from(PROJECT_QUALIFIER, PROJECT_ORGANIZATION, PROJECT_NAME)
}

/// Diretório de dados do aplicativo (biblioteca local, índices persistentes).
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_dir().to_owned())
}

/// Diretório de configuração do usuário (onde vive `settings.toml`).
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.config_dir().to_owned())
}

/// Diretório de cache dispensável (artwork remota, prévias temporárias).
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.cache_dir().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Em Linux com $HOME definido os caminhos XDG derivam sem erro.
    #[test]
    fn paths_resolve_on_linux() {
        if std::env::var_os("HOME").is_none() {
            return;
        }
        let config = config_dir().expect("caminhos XDG deriváveis com HOME definido");
        assert!(config.ends_with("sitrus"));
    }
}
