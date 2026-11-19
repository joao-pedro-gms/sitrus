//! Design tokens do Sitrus (documentados em `docs/design-system.md`).
//!
//! Nenhum valor visual deve aparecer solto na tela: componentes leem
//! espaçamentos, raios, alturas e a escada tipográfica daqui.
//! Escala de espaçamento com base 4; raios 6/10/14 e o raio de pílula;
//! alturas das zonas estruturais; escada tipográfica Display → Caption.

/// Escala de espaçamento com base 4 (mais o meio-passo `X2`).
pub mod spacing {
    /// Meio-passo, para ajustes finos (badges, bordas de ícone).
    pub const X2: f32 = 2.0;
    /// Base da escala (`4`).
    pub const X4: f32 = 4.0;
    pub const X8: f32 = 8.0;
    pub const X12: f32 = 12.0;
    pub const X16: f32 = 16.0;
    pub const X20: f32 = 20.0;
    pub const X24: f32 = 24.0;
    pub const X32: f32 = 32.0;
    pub const X40: f32 = 40.0;
    pub const X56: f32 = 56.0;

    /// Todos os passos da escala, na ordem.
    pub const SCALE: [f32; 10] = [X2, X4, X8, X12, X16, X20, X24, X32, X40, X56];
}

/// Raios de canto: `sm` card de lista, `md` artwork/botão, `lg` painéis,
/// `pill` para pílulas e cantos totalmente arredondados.
pub mod radius {
    /// Cartões pequenos e listas.
    pub const SM: f32 = 6.0;
    /// Artwork, botões, campos.
    pub const MD: f32 = 10.0;
    /// Painéis elevados, hero, player.
    pub const LG: f32 = 14.0;
    /// Pílulas (botão primário, segmented control, slider).
    pub const PILL: f32 = 999.0;

    pub const ALL: [f32; 4] = [SM, MD, LG, PILL];
}

/// Alturas e tamanhos estruturais (px lógicos).
pub mod sizes {
    /// Linha de lista densa.
    pub const LIST_ROW: f32 = 44.0;
    /// Item do rail de navegação.
    pub const RAIL_ITEM: f32 = 40.0;
    /// Card de prateleira (artwork 180 + título + subtítulo).
    pub const SHELF_CARD: f32 = 224.0;
    /// Artwork do prateleira dentro do card.
    pub const SHELF_ARTWORK: f32 = 180.0;
    /// Artwork e altura do hero.
    pub const HERO: f32 = 320.0;
    /// Barra de player fixa no rodapé.
    pub const PLAYER_BAR: f32 = 88.0;
    /// Toolbar superior.
    pub const TOOLBAR: f32 = 52.0;
    /// Alvo de clique mínimo (referência macOS).
    pub const MIN_CLICK_TARGET: f32 = 24.0;
}

/// Peso tipográfico (mapeado para o eixo `wght` da Inter variável).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weight {
    Regular = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
}

impl Weight {
    /// Valor numérico do eixo `wght`.
    #[must_use]
    pub const fn value(self) -> f32 {
        match self {
            Weight::Regular => 400.0,
            Weight::Medium => 500.0,
            Weight::SemiBold => 600.0,
            Weight::Bold => 700.0,
        }
    }
}

/// Um nível da escada tipográfica: tamanho, altura de linha e peso.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeSpec {
    /// Corpo em px.
    pub size: f32,
    /// Altura de linha em px.
    pub line_height: f32,
    /// Peso.
    pub weight: Weight,
}

/// Alias estável para os tokens tipográficos (`crate::tokens::type_scale`).
pub mod type_scale {
    pub use super::{TypeSpec, TypeToken, Weight};
}

/// Tokens da escada tipográfica (equivalente sugerido à escada SF).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeToken {
    Display,
    LargeTitle,
    Title1,
    Title2,
    Headline,
    Body,
    Subheadline,
    Footnote,
    Caption,
}

impl TypeToken {
    /// Especificação completa do nível.
    #[must_use]
    pub const fn spec(self) -> TypeSpec {
        let (size, line_height, weight) = match self {
            TypeToken::Display => (44.0, 52.0, Weight::Bold),
            TypeToken::LargeTitle => (34.0, 41.0, Weight::SemiBold),
            TypeToken::Title1 => (28.0, 34.0, Weight::SemiBold),
            TypeToken::Title2 => (22.0, 28.0, Weight::SemiBold),
            TypeToken::Headline => (17.0, 22.0, Weight::SemiBold),
            TypeToken::Body => (17.0, 22.0, Weight::Regular),
            TypeToken::Subheadline => (15.0, 20.0, Weight::Regular),
            TypeToken::Footnote => (13.0, 18.0, Weight::Regular),
            TypeToken::Caption => (12.0, 16.0, Weight::Medium),
        };
        TypeSpec {
            size,
            line_height,
            weight,
        }
    }

    /// Todos os níveis, do maior para o menor.
    #[must_use]
    pub fn all() -> &'static [TypeToken] {
        &[
            TypeToken::Display,
            TypeToken::LargeTitle,
            TypeToken::Title1,
            TypeToken::Title2,
            TypeToken::Headline,
            TypeToken::Body,
            TypeToken::Subheadline,
            TypeToken::Footnote,
            TypeToken::Caption,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Todo passo da escala é múltiplo de 2 e, acima de 2, múltiplo de 4.
    #[test]
    fn spacing_scale_is_base_4() {
        for step in spacing::SCALE {
            assert_eq!(step % 2.0, 0.0, "passo {step} não é múltiplo de 2");
            if step > spacing::X2 {
                assert_eq!(step % 4.0, 0.0, "passo {step} não é múltiplo de 4");
            }
        }
        let values: Vec<f32> = spacing::SCALE.to_vec();
        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("sem NaN na escala"));
        assert_eq!(values, sorted, "escala deve ser crescente");
    }

    /// Raios na ordem esperada, com `pill` no topo.
    #[test]
    fn radius_order() {
        const { assert!(radius::SM < radius::MD) }
        const { assert!(radius::MD < radius::LG) }
        const { assert!(radius::LG < radius::PILL) }
        assert_eq!(radius::SM, 6.0);
        assert_eq!(radius::MD, 10.0);
        assert_eq!(radius::LG, 14.0);
        assert_eq!(radius::PILL, 999.0);
    }

    /// Alturas estruturais do design system.
    #[test]
    fn structural_sizes_match_design_system() {
        assert_eq!(sizes::LIST_ROW, 44.0);
        assert_eq!(sizes::RAIL_ITEM, 40.0);
        assert_eq!(sizes::SHELF_CARD, 224.0);
        assert_eq!(sizes::SHELF_ARTWORK, 180.0);
        assert_eq!(sizes::HERO, 320.0);
        assert_eq!(sizes::PLAYER_BAR, 88.0);
        assert_eq!(sizes::TOOLBAR, 52.0);
        assert_eq!(sizes::MIN_CLICK_TARGET, 24.0);
    }

    /// A escada tipográfica desce de 44 (Display) até 12 (Caption),
    /// altura de linha sempre maior que o corpo, pesos coerentes.
    #[test]
    fn type_ladder_descends_from_display_to_caption() {
        let ladder = TypeToken::all();
        assert_eq!(ladder.len(), 9);
        let mut previous = f32::MAX;
        for token in ladder {
            let spec = token.spec();
            assert!(
                spec.size <= previous,
                "escada cresce de {previous} para {}",
                spec.size
            );
            previous = spec.size;
            assert!(spec.line_height > spec.size, "linha < corpo em {token:?}");
            assert!(spec.size / 2.0 <= spec.line_height);
        }
        assert_eq!(ladder[0].spec().size, 44.0, "Display deve ter 44 px");
        assert_eq!(ladder[8].spec().size, 12.0, "Caption deve ter 12 px");
    }

    /// Pesos tipográficos batem com o eixo `wght` da Inter.
    #[test]
    fn weights_map_to_wght_axis() {
        assert_eq!(Weight::Regular.value(), 400.0);
        assert_eq!(Weight::Medium.value(), 500.0);
        assert_eq!(Weight::SemiBold.value(), 600.0);
        assert_eq!(Weight::Bold.value(), 700.0);
        assert!(TypeToken::Display.spec().weight.value() > TypeToken::Body.spec().weight.value());
    }
}
