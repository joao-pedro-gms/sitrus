//! Fila de reprodução sem repetição automática.

use crate::model::Song;

/// Fila ordenada com cursor opcional para a faixa atual.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaybackQueue {
    songs: Vec<Song>,
    current_index: Option<usize>,
}

impl PlaybackQueue {
    /// Cria uma fila vazia.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            songs: Vec::new(),
            current_index: None,
        }
    }

    /// Insere a faixa imediatamente após a faixa atual, ou no início se não houver atual.
    pub fn add_next(&mut self, song: Song) {
        let index = self.current_index.map_or(0, |current| current + 1);
        self.songs.insert(index, song);
    }

    /// Acrescenta uma faixa ao fim da fila.
    pub fn add_last(&mut self, song: Song) {
        self.songs.push(song);
    }

    /// Remove e devolve a faixa no índice informado, se existir.
    pub fn remove(&mut self, index: usize) -> Option<Song> {
        if index >= self.songs.len() {
            return None;
        }

        let removed = self.songs.remove(index);
        self.current_index = match self.current_index {
            Some(current) if index < current => Some(current - 1),
            Some(current) if index == current => {
                if self.songs.is_empty() {
                    None
                } else {
                    Some(index.min(self.songs.len() - 1))
                }
            }
            current => current,
        };
        Some(removed)
    }

    /// Move uma faixa para outro índice; índices inválidos não alteram a fila.
    pub fn reorder(&mut self, from: usize, to: usize) -> Option<()> {
        if from >= self.songs.len() || to >= self.songs.len() {
            return None;
        }
        if from == to {
            return Some(());
        }

        if let Some(current) = self.current_index {
            self.current_index = if current == from {
                Some(to)
            } else if from < current && to >= current {
                Some(current - 1)
            } else if from > current && to <= current {
                Some(current + 1)
            } else {
                Some(current)
            };
        }

        let song = self.songs.remove(from);
        self.songs.insert(to, song);
        Some(())
    }

    /// Seleciona uma faixa pelo índice; índices inválidos não alteram o cursor.
    pub fn move_to(&mut self, index: usize) -> Option<&Song> {
        let song = self.songs.get(index)?;
        self.current_index = Some(index);
        Some(song)
    }

    /// Seleciona a primeira faixa ou avança; no fim da fila retorna `None` sem repetir.
    /// O nome segue a operação de domínio solicitada, não a semântica de `Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&Song> {
        match self.current_index {
            None if !self.songs.is_empty() => self.current_index = Some(0),
            Some(current) if current + 1 < self.songs.len() => {
                self.current_index = Some(current + 1);
            }
            _ => return None,
        }
        self.current()
    }

    /// Volta uma faixa; no início (ou antes da primeira seleção) retorna `None`.
    pub fn previous(&mut self) -> Option<&Song> {
        match self.current_index {
            Some(current) if current > 0 => self.current_index = Some(current - 1),
            _ => return None,
        }
        self.current()
    }

    /// Retorna a faixa selecionada, se houver.
    #[must_use]
    pub fn current(&self) -> Option<&Song> {
        self.current_index.and_then(|index| self.songs.get(index))
    }

    /// Índice selecionado, quando a fila tem faixa corrente.
    #[must_use]
    pub const fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    /// Número de faixas na fila.
    #[must_use]
    pub fn len(&self) -> usize {
        self.songs.len()
    }

    /// Indica se a fila está vazia.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seleção por índice válido troca o cursor para a faixa pedida.
    #[test]
    fn move_to_selects_valid_index() {
        let mut queue = test_queue();
        assert_eq!(queue.move_to(1).map(|song| song.id.as_str()), Some("b"));
        assert_eq!(queue.current().map(|song| song.id.as_str()), Some("b"));
    }

    /// Índices fora da fila não mudam a seleção atual.
    #[test]
    fn move_to_rejects_invalid_index_without_changing_cursor() {
        let mut queue = test_queue();
        queue.move_to(0);
        assert!(queue.move_to(3).is_none());
        assert_eq!(queue.current().map(|song| song.id.as_str()), Some("a"));
    }

    /// Os índices extremos selecionam a primeira e a última faixa.
    #[test]
    fn move_to_handles_queue_extremes() {
        let mut queue = test_queue();
        assert_eq!(queue.move_to(0).map(|song| song.id.as_str()), Some("a"));
        assert_eq!(queue.move_to(2).map(|song| song.id.as_str()), Some("c"));
    }

    fn test_queue() -> PlaybackQueue {
        use crate::model::{EntityId, Song};

        let mut queue = PlaybackQueue::new();
        for id in ["a", "b", "c"] {
            queue.add_last(Song::new(EntityId::new(id), id, EntityId::new("album")));
        }
        queue
    }
}
