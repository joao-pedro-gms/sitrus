//! Erros do núcleo do Sitrus.

/// Erros levantados pela configuração e utilidades do núcleo.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Falha de I/O ao ler ou gravar arquivos do usuário.
    #[error("falha de e/s: {0}")]
    Io(#[from] std::io::Error),

    /// O arquivo de configuração não pôde ser interpretado como TOML.
    #[error("configuração inválida: {0}")]
    InvalidToml(#[from] toml::de::Error),

    /// As configurações atuais não puderam ser serializadas.
    #[error("falha ao serializar configuração: {0}")]
    SerializeToml(#[from] toml::ser::Error),

    /// Não foi possível determinar um diretório de configuração do usuário.
    #[error("diretório de configuração do usuário indisponível")]
    ConfigDirectoryUnavailable,
}
