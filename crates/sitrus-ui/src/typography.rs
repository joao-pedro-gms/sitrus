//! Tipografia: fonte Inter variável embutida (SIL OFL 1.1) e a escada de
//! estilos de `docs/design-system.md`.
//!
//! SF Pro não pode ser redistribuído; o app embute a Inter de `rsms/inter`
//! (métricas próximas, licença OFL 1.1, arquivo copiado em `assets/fonts/`).
//! egui 0.37 lê fontes variáveis com `FontTweak::coords`, então um único
//! `InterVariable.ttf` fornece os pesos 400/500/600/700.
use egui::epaint::text::{FontTweak, VariationCoords};
use egui::{Context, FontData, FontDefinitions, FontFamily, FontId};

use crate::tokens::{TypeToken, Weight};

/// Bytes da fonte variável embutida no binário.
pub static INTER_VARIABLE: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");

/// Nome do arquivo e licença da fonte embutida (para `--version`/handoff).
pub const FONT_SOURCE: &str = "InterVariable.ttf (Inter Variable, rsms/inter v4.1, SIL OFL 1.1)";

/// Familias registradas no `FontDefinitions` — nomes-chaves estáveis.
pub const FAMILY_REGULAR: &str = "sitrus_inter_w400";
pub const FAMILY_MEDIUM: &str = "sitrus_inter_w500";
pub const FAMILY_SEMI_BOLD: &str = "sitrus_inter_w600";
pub const FAMILY_BOLD: &str = "sitrus_inter_w700";

fn family_name(weight: Weight) -> &'static str {
    match weight {
        Weight::Regular => FAMILY_REGULAR,
        Weight::Medium => FAMILY_MEDIUM,
        Weight::SemiBold => FAMILY_SEMI_BOLD,
        Weight::Bold => FAMILY_BOLD,
    }
}

/// Monta o `FontDefinitions` completo: Inter primeiro para `Pro portional`,
/// fontes padrão do egui como fallback (glifos extras), e uma família nomeada
/// por peso (Display/Bold etc. usam os pesos reais da fonte variável).
#[must_use]
pub fn font_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for weight in [
        Weight::Regular,
        Weight::Medium,
        Weight::SemiBold,
        Weight::Bold,
    ] {
        let data = FontData::from_static(INTER_VARIABLE).tweak(FontTweak {
            coords: VariationCoords::new([(b"wght", weight.value())]),
            ..FontTweak::default()
        });
        // A chave dos dados é o próprio nome da família (um rosto por peso).
        let key = family_name(weight).to_owned();
        defs.font_data
            .insert(key.clone(), std::sync::Arc::new(data));
        defs.families
            .entry(FontFamily::Name(key.clone().into()))
            .or_default()
            .push(key.clone());
        if weight == Weight::Regular {
            // Inter fica no topo da família padrão; fallbacks do egui seguem.
            if let Some(list) = defs.families.get_mut(&FontFamily::Proportional) {
                list.insert(0, key);
            }
        }
    }
    defs
}

/// Instala a tipografia no `Context` (uma vez, na primeira janela).
pub fn install(ctx: &Context) {
    ctx.set_fonts(font_definitions());
}

/// Instala a tipografia apenas uma vez por `Context` (idempotente),
/// para uso em app e galeria no mesmo processo.
pub fn install_once(ctx: &Context) {
    let installed = ctx
        .data(|d| d.get_temp::<bool>(egui::Id::unique("sitrus_typography_installed")))
        .unwrap_or(false);
    if !installed {
        install(ctx);
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::unique("sitrus_typography_installed"), true);
        });
    }
}

/// `FontId` completo para um nível da escada: tamanho do token com a
/// família (peso) correspondente.
#[must_use]
pub fn font_id(token: TypeToken) -> FontId {
    let spec = token.spec();
    FontId::new(spec.size, FontFamily::Name(family_name(spec.weight).into()))
}

/// Familia egui para um peso arbitrário da escada.
#[must_use]
pub fn family(weight: Weight) -> FontFamily {
    FontFamily::Name(family_name(weight).into())
}

/// Lista de-fontes registradas (documentação/`--gallery`).
#[must_use]
pub fn registered_families() -> &'static [&'static str] {
    &[FAMILY_REGULAR, FAMILY_MEDIUM, FAMILY_SEMI_BOLD, FAMILY_BOLD]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As quatro famílias de peso são registradas com o eixo `wght` correto.
    #[test]
    fn registers_four_weight_families() {
        let defs = font_definitions();
        for family in registered_families() {
            let key = FontFamily::Name((*family).into());
            let faces = defs.families.get(&key).expect("família registrada");
            assert_eq!(faces.len(), 1, "família {family} deve ter um rosto");
        }
        // Cada rosto guarda o tweak com coords de peso.
        for (name, data) in &defs.font_data {
            if name.starts_with("sitrus_inter") {
                assert_eq!(data.bytes().len(), INTER_VARIABLE.len(), "bytes da Inter");
            }
        }
    }

    /// Inter é a fonte primária de `Pro portional`; fallbacks do egui ficam depois.
    #[test]
    fn inter_leads_proportional_family() {
        let defs = font_definitions();
        let proportional = defs
            .families
            .get(&FontFamily::Proportional)
            .expect("família padrão existe");
        assert_eq!(
            proportional.first().map(String::as_str),
            Some(FAMILY_REGULAR),
            "Inter deve ser a primeira fonte de Pro portional"
        );
        assert!(proportional.len() > 1, "fallbacks do egui devem permanecer");
    }

    /// `font_id` mapeia cada token para tamanho e família do peso certo.
    #[test]
    fn font_id_matches_type_ladder() {
        for token in TypeToken::all() {
            let spec = token.spec();
            let id = font_id(*token);
            assert_eq!(id.size, spec.size, "tamanho de {token:?}");
            match spec.weight {
                Weight::Regular => assert_eq!(id.family, family(Weight::Regular)),
                Weight::Medium => assert_eq!(id.family, family(Weight::Medium)),
                Weight::SemiBold => assert_eq!(id.family, family(Weight::SemiBold)),
                Weight::Bold => assert_eq!(id.family, family(Weight::Bold)),
            }
        }
        // Display usa Bold, Body usa Regular, Caption usa Medium.
        assert_eq!(
            font_id(TypeToken::Display).family,
            FontFamily::Name(FAMILY_BOLD.into())
        );
        assert_eq!(
            font_id(TypeToken::Body).family,
            FontFamily::Name(FAMILY_REGULAR.into())
        );
        assert_eq!(
            font_id(TypeToken::Caption).family,
            FontFamily::Name(FAMILY_MEDIUM.into())
        );
    }

    /// Guarda a fonte embutida não-vazia e documentada.
    #[test]
    fn embedded_font_and_license_documented() {
        assert!(
            INTER_VARIABLE.len() > 100_000,
            "fonte embutida deve ser real"
        );
        assert!(FONT_SOURCE.contains("OFL"));
    }
}
