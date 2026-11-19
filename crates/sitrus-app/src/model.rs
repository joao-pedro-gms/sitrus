//! Tipos de interface compartilhados entre as telas do aplicativo.
//!
//! As linhas são projeções dos modelos da `sitrus-api` (biblioteca) e do
//! catálogo (busca), levando já a `Song` de domínio pronta para a fila.

use std::time::Duration;

/// Linha de faixa exibível em listas, prateleiras, busca e fila.
#[derive(Debug, Clone, PartialEq)]
pub struct SongRow {
    /// Identificador de exibição (id da Apple).
    pub id: String,
    /// Título.
    pub title: String,
    /// Subtítulo de exibição (artista ou álbum, conforme a origem).
    pub subtitle: String,
    /// Nome do álbum, quando conhecido.
    pub album_name: String,
    /// Duração em segundos (0 quando desconhecida).
    pub duration_secs: u32,
    /// URL de capa (com `{w}`/`{h}`) quando disponível.
    pub artwork_url: Option<String>,
    /// URL de reprodução quando a origem a fornece (prévia do catálogo).
    pub preview_url: Option<url::Url>,
    /// Faixa de domínio correspondente, pronta para a fila de reprodução.
    pub song: sitrus_core::model::Song,
}

/// Linha de playlist exibível na tela Playlists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    /// Identificador da Apple.
    pub id: String,
    /// Nome de exibição.
    pub name: String,
    /// Quantidade de faixas, quando a resposta a traz.
    pub track_count: Option<u64>,
    /// URL de capa (com `{w}`/`{h}`), quando disponível.
    pub artwork_url: Option<String>,
}

/// Estado de carregamento de uma seção de dados remotos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SectionState {
    /// Aguardando resposta, mostra skeletons.
    Loading,
    /// Coleta concluída (linhas vazias viram estado vazio na UI).
    #[default]
    Loaded,
    /// Falha na coleta, com mensagem pronta para o banner.
    Failed(String),
}

impl SectionState {
    /// `true` quando a seção está no ciclo de carregamento.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// Modo de apresentação de biblioteca/playlists (prateleiras ou lista).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMode {
    /// Grade de cards (prateleiras).
    Shelves,
    /// Linhas densas de lista.
    List,
}

impl ListMode {
    /// Rótulos do controle segmentado, na ordem.
    pub const SEGMENT_LABELS: [&'static str; 2] = ["Prateleiras", "Lista"];

    /// Índice do modo no controle segmentado.
    #[must_use]
    pub const fn segment_index(self) -> usize {
        match self {
            Self::Shelves => 0,
            Self::List => 1,
        }
    }
}

/// Converte milissegundos em segundos truncados.
#[must_use]
pub fn secs_from_millis(millis: u64) -> u32 {
    u32::try_from(millis / 1000).unwrap_or(u32::MAX)
}

/// `Duration` em segundos truncados para os rótulos da barra.
#[must_use]
pub fn secs_from_duration(value: Duration) -> u32 {
    u32::try_from(value.as_secs().min(u64::from(u32::MAX))).unwrap_or(u32::MAX)
}
