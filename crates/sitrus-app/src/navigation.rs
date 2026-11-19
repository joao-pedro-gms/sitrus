//! Pilha de navegação entre telas: funções puras sobre o `Screen` corrente e
//! a pilha de histórico. As telas de detalhe (artista, álbum, playlist)
//! empilham a origem; o rail e o atalho Ctrl+F voltam à raiz.
//!
//! Testável sem contexto gráfico: nenhuma função toca egui ou canal.

use crate::app::Screen;

/// Empurra a tela corrente para o histórico e entra em `next`.
pub fn nav_push(current: &mut Screen, stack: &mut Vec<Screen>, next: Screen) {
    stack.push(current.clone());
    *current = next;
}

/// Volta uma entrada do histórico (`true` quando houve retorno).
pub fn nav_pop(current: &mut Screen, stack: &mut Vec<Screen>) -> bool {
    match stack.pop() {
        Some(previous) => {
            *current = previous;
            true
        }
        None => false,
    }
}

/// Volta à raiz (rail/atalho): descarta o histórico e troca de seção.
pub fn nav_reset(current: &mut Screen, stack: &mut Vec<Screen>, root: Screen) {
    stack.clear();
    *current = root;
}

/// Há tela para voltar na pilha?
#[must_use]
pub fn can_pop(stack: &[Screen]) -> bool {
    !stack.is_empty()
}

/// `true` quando a tela é uma página de detalhe com pilha de retorno.
#[must_use]
pub fn is_detail(screen: &Screen) -> bool {
    matches!(
        screen,
        Screen::Artist(_) | Screen::Album(_) | Screen::Playlist(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abrir um detalhe a partir da Busca empilha a Busca e volta para ela.
    #[test]
    fn search_to_artist_and_back() {
        let mut current = Screen::Search;
        let mut stack = Vec::new();
        nav_push(&mut current, &mut stack, Screen::Artist("100".to_owned()));
        assert!(is_detail(&current));
        assert!(can_pop(&stack));
        assert!(nav_pop(&mut current, &mut stack));
        assert_eq!(current, Screen::Search);
        assert!(!can_pop(&stack));
    }

    /// Cadeia artista → álbum empilha níveis; dois pop devolvem à raiz.
    #[test]
    fn artist_to_album_stack() {
        let mut current = Screen::Search;
        let mut stack = Vec::new();
        nav_push(&mut current, &mut stack, Screen::Artist("7".to_owned()));
        nav_push(&mut current, &mut stack, Screen::Album("42".to_owned()));
        assert_eq!(current, Screen::Album("42".to_owned()));
        assert!(nav_pop(&mut current, &mut stack));
        assert_eq!(current, Screen::Artist("7".to_owned()));
        assert!(nav_pop(&mut current, &mut stack));
        assert_eq!(current, Screen::Search);
        assert!(!nav_pop(&mut current, &mut stack));
        assert_eq!(current, Screen::Search);
    }

    /// Playlists → detalhe de playlist → raiz pelo rail limpa a pilha.
    #[test]
    fn rail_reset_clears_history() {
        let mut current = Screen::Playlists;
        let mut stack = Vec::new();
        nav_push(&mut current, &mut stack, Screen::Playlist("p.1".to_owned()));
        assert!(can_pop(&stack));
        nav_reset(&mut current, &mut stack, Screen::Home);
        assert_eq!(current, Screen::Home);
        assert!(!can_pop(&stack));
    }

    /// Detalhe reconhecido por variante; raízes de rail não empilham volta.
    #[test]
    fn detail_detection_and_pop_on_empty_stack() {
        assert!(!is_detail(&Screen::Home));
        assert!(!is_detail(&Screen::Library));
        assert!(!is_detail(&Screen::Login));
        assert!(is_detail(&Screen::Playlist("x".to_owned())));
        let mut current = Screen::Home;
        let mut stack = Vec::new();
        assert!(!nav_pop(&mut current, &mut stack));
        assert_eq!(current, Screen::Home);
    }
}
