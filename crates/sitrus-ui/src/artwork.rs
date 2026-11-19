//! Artwork: decodificação, cores dominantes, gradiente de hero e blur em
//! software (wayland/egui não expõe blur de compositor — o substituto é
//! downscale 32×32 → blur gaussiano → upscale com o crate `image`).

use std::collections::HashMap;

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use image::{DynamicImage, GenericImageView};

use crate::theme::tint;

/// Resolução do downscale usado pelo blur e pela extração de cores.
pub const BLUR_DOWNSCALE: u32 = 32;
/// Sigma do blur gaussiano (`image::imageops::blur`).
pub const BLUR_SIGMA: f32 = 7.0;
/// Contraste mínimo do texto sobre o hero (texto grande: 3:1; usamos 4.5 por
/// segurança — o véu escuro do hero cobre o restante).
pub const MIN_HERO_CONTRAST: f32 = 4.5;

/// Buckets de cor quantizada (3 bits/canal) e a média RGB dos seus pixels.
type PixelBuckets = HashMap<(u8, u8, u8), Vec<(f32, f32, f32)>>;

/// As duas cores dominantes de uma capa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DominantColors {
    /// Cor dominante (maior área/peso).
    pub primary: Color32,
    /// Segunda cor dominante, de matiz distante da primeira.
    pub secondary: Color32,
}

/// Decodifica bytes de imagem (PNG/JPEG/...) ou devolve `None` se inválidos.
#[must_use]
pub fn decode_artwork(bytes: &[u8]) -> Option<DynamicImage> {
    // `image` escolhe o codec pelos magic bytes; artwork é opcional, então
    // erros de decodificação viram `None` em vez de travar o app.
    image::load_from_memory(bytes).ok()
}

/// Extrai as duas cores dominantes.
///
/// Algoritmo determinístico: reduz a imagem a no máximo 32×32, quantiza cada
/// canal em 3 bits (512 buckets) e pontua cada bucket por
/// `área × (0.2 + saturação)`; a cor secundária exige matiz distante (≥ 40°)
/// da primária e, sem candidato, usa o bucket mais saturado restante.
#[must_use]
pub fn dominant_colors(img: &DynamicImage) -> DominantColors {
    let thumb = img.thumbnail(BLUR_DOWNSCALE, BLUR_DOWNSCALE);
    let (w, h) = thumb.dimensions();

    let mut buckets: PixelBuckets = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let px = thumb.get_pixel(x, y);
            if px[3] < 128 {
                continue; // quase transparente não pinta o hero
            }
            let key = (px[0] >> 5, px[1] >> 5, px[2] >> 5);
            buckets.entry(key).or_default().push((
                f32::from(px[0]),
                f32::from(px[1]),
                f32::from(px[2]),
            ));
        }
    }

    #[derive(Clone)]
    struct BucketScore {
        avg: Color32,
        score: f32,
        hue: f32,
        saturation: f32,
    }

    let mut scored: Vec<BucketScore> = buckets
        .values()
        .filter(|pixels| !pixels.is_empty())
        .map(|pixels| {
            let n = pixels.len() as f32;
            let avg = pixels.iter().fold((0.0, 0.0, 0.0), |acc, px| {
                (acc.0 + px.0, acc.1 + px.1, acc.2 + px.2)
            });
            let avg = Color32::from_rgb(
                (avg.0 / n).round() as u8,
                (avg.1 / n).round() as u8,
                (avg.2 / n).round() as u8,
            );
            let (hue, saturation) = hue_saturation(avg);
            BucketScore {
                avg,
                score: n * (0.2 + saturation),
                hue,
                saturation,
            }
        })
        .collect();
    // Empates (ex.: capas bicolor) têm vencedor determinístico por cor média.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| (b.avg.g(), b.avg.r(), b.avg.b()).cmp(&(a.avg.g(), a.avg.r(), a.avg.b())))
    });

    let Some(primary) = scored.first().cloned() else {
        // Não houve pixels utilizáveis: cinza neutro para não gerar contraste ruim.
        return DominantColors {
            primary: Color32::from_rgb(0x48, 0x48, 0x50),
            secondary: Color32::from_rgb(0x2A, 0x2A, 0x30),
        };
    };

    let secondary = scored
        .iter()
        .skip(1)
        .find(|b| hue_distance(b.hue, primary.hue) >= 40.0)
        .map(|b| b.avg)
        .or_else(|| {
            scored
                .iter()
                .skip(1)
                .max_by(|a, b| {
                    a.saturation
                        .partial_cmp(&b.saturation)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|b| b.avg)
        })
        .unwrap_or_else(|| tint(primary.avg, Color32::WHITE, 0.35));

    DominantColors {
        primary: primary.avg,
        secondary,
    }
}

/// Gradiente de hero extraído do artwork, com as duas cores escurecidas até
/// garantir contraste de texto branco (`docs/design-system.md`).
#[must_use]
pub fn hero_gradient(img: &DynamicImage) -> (Color32, Color32) {
    let DominantColors { primary, secondary } = dominant_colors(img);
    // `from` leva mais do primário, `to` mais do secundário; ambos escuros.
    let from = darken_for_contrast(tint(primary, Color32::BLACK, 0.15), MIN_HERO_CONTRAST);
    let to = darken_for_contrast(tint(secondary, Color32::BLACK, 0.35), MIN_HERO_CONTRAST);
    (from, to)
}

/// Escurece a cor (mistura com preto progressiva) até contrastar `min_ratio`
/// contra branco. Determinístico; hue é preservado na prática.
#[must_use]
pub fn darken_for_contrast(color: Color32, min_ratio: f32) -> Color32 {
    let mut current = color;
    let mut steps = 0;
    while contrast_ratio(current, Color32::WHITE) < min_ratio && steps < 16 {
        current = tint(current, Color32::BLACK, 0.12);
        steps += 1;
    }
    current
}

/// Contraste WCAG entre duas cores opacas (1.0..21.0).
#[must_use]
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// Luminância relativa sRGB (WCAG 2.x).
#[must_use]
pub fn relative_luminance(color: Color32) -> f32 {
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.212_6 * linear(color.r()) + 0.715_2 * linear(color.g()) + 0.072_2 * linear(color.b())
}

/// Matiz (graus 0..360) e saturação (0..1) de uma cor opaca.
#[must_use]
pub fn hue_saturation(color: Color32) -> (f32, f32) {
    let (r, g, b) = (
        f32::from(color.r()) / 255.0,
        f32::from(color.g()) / 255.0,
        f32::from(color.b()) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let saturation = if max <= 0.0 { 0.0 } else { delta / max };
    if delta <= 0.0 {
        return (0.0, saturation);
    }
    let hue = if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), saturation)
}

/// Distância angular entre matizes (0..180).
#[must_use]
fn hue_distance(a: f32, b: f32) -> f32 {
    let diff = (a - b).abs().rem_euclid(360.0);
    diff.min(360.0 - diff)
}

/// Fundo desfocado para hero/painéis: downscale 32×32 → blur gaussiano →
/// upscale suave (o "vidro" da direção B, em software).
#[must_use]
pub fn blurred_hero_background(img: &DynamicImage) -> DynamicImage {
    let thumb = img.resize_exact(
        BLUR_DOWNSCALE,
        BLUR_DOWNSCALE,
        image::imageops::FilterType::Triangle,
    );
    let blurred = image::imageops::blur(&thumb, BLUR_SIGMA);
    let out = DynamicImage::from(blurred);
    out.resize_exact(512, 512, image::imageops::FilterType::Triangle)
}

/// Converte para textura egui RGB (unmultiplied).
#[must_use]
pub fn to_color_image(img: &DynamicImage) -> ColorImage {
    let size = [img.width() as usize, img.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, &img.to_rgba8().into_raw())
}

/// Carrega um artwork como textura egui nomeada.
pub fn load_texture(ctx: &Context, name: &str, img: &DynamicImage) -> TextureHandle {
    ctx.load_texture(name, to_color_image(img), TextureOptions::LINEAR)
}

/// Artwork sintético determinístico (capas de demonstração da galeria).
#[must_use]
pub fn sample_artwork(seed: u32, size: u32) -> DynamicImage {
    let mix = seed.wrapping_mul(2_654_435_761);
    let base_h = u8::try_from((mix >> 8) & 0xFF).unwrap_or(180);
    let base_l = u8::try_from((mix >> 16) & 0xFF).unwrap_or(96);
    let a = Color32::from_rgb(
        (base_h / 2).saturating_add(30),
        (base_l / 2).saturating_add(60),
        base_h.saturating_add(90),
    );
    let b = Color32::from_rgb(
        base_l.saturating_add(90),
        (base_h / 3).saturating_add(40),
        (base_l / 2).saturating_add(70),
    );
    let cx = size as f32 * (0.25 + f32::from(u8::try_from(mix & 0xFF).unwrap_or(64)) / 400.0);
    let cy =
        size as f32 * (0.30 + f32::from(u8::try_from((mix >> 4) & 0xFF).unwrap_or(72)) / 400.0);
    let r1 = size as f32 * 0.42;
    let r2 = size as f32 * 0.22;

    let mut img = image::RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let fx = x as f32;
        let fy = y as f32;
        let t = fy / (size as f32).max(1.0);
        let mut r = f32::from(a.r()) * (1.0 - t) + f32::from(b.r()) * t;
        let mut g = f32::from(a.g()) * (1.0 - t) + f32::from(b.g()) * t;
        let mut bl = f32::from(a.b()) * (1.0 - t) + f32::from(b.b()) * t;
        for (ccx, ccy, radius, weight) in
            [(cx, cy, r1, 0.35), (cx + r1 * 0.55, cy + r1 * 0.3, r2, 0.5)]
        {
            let d = ((fx - ccx).powi(2) + (fy - ccy).powi(2)).sqrt();
            if d < radius {
                let k = weight * (1.0 - d / radius);
                r = r * (1.0 - k) + f32::from(b.r()) * k;
                g = g * (1.0 - k) + f32::from(b.g()) * k;
                bl = bl * (1.0 - k) + f32::from(b.b()) * k;
            }
        }
        *px = image::Rgba([r.round() as u8, g.round() as u8, bl.round() as u8, 255]);
    }
    DynamicImage::ImageRgba8(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    /// Um PNG de 1x1 pixel gerado pelo próprio `image` decodifica com sucesso.
    #[test]
    fn decodes_valid_png() {
        let img = DynamicImage::new_rgb8(1, 1);
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .expect("codificação PNG de 1x1");
        assert!(decode_artwork(&png_bytes).is_some());
    }

    /// Bytes aleatórios não decodificam e devolvem `None`.
    #[test]
    fn rejects_garbage_bytes() {
        assert!(decode_artwork(&[0x00, 0x01, 0x02, 0x03]).is_none());
    }

    /// Imagem sintética metade vermelha / metade azul: a cor dominante é
    /// vermelha e a secundária é azul de matiz distante.
    #[test]
    fn dominant_colors_from_two_tone_synthetic() {
        let red = Color32::from_rgb(220, 30, 40);
        let blue = Color32::from_rgb(30, 60, 220);
        let img = two_tone_weighted(red, blue, 64);
        let colors = dominant_colors(&img);
        let hue_p = hue_saturation(colors.primary).0;
        let hue_s = hue_saturation(colors.secondary).0;
        assert!(
            colors.primary.r() > 150
                && colors.primary.r() > colors.primary.g() + 50
                && colors.primary.r() > colors.primary.b() + 50,
            "primary vermelha: {:?}",
            colors.primary
        );
        assert!(
            colors.secondary.b() > 150
                && colors.secondary.b() > colors.secondary.r() + 50
                && colors.secondary.b() > colors.secondary.g() + 40,
            "secondary azul: {:?}",
            colors.secondary
        );
        assert!(
            hue_distance(hue_p, hue_s) >= 40.0,
            "matizes distantes {hue_p} vs {hue_s}"
        );
        assert_ne!(colors.primary, colors.secondary);
    }

    /// `hero_gradient` sempre devolve cores escuras o bastante para texto
    /// branco e mantém matiz da capa.
    #[test]
    fn hero_gradient_is_dark_and_text_readaable() {
        let red = Color32::from_rgb(220, 30, 40);
        let blue = Color32::from_rgb(30, 60, 220);
        let (from, to) = hero_gradient(&two_tone_weighted(red, blue, 64));
        for color in [from, to] {
            assert!(
                contrast_ratio(color, Color32::WHITE) >= MIN_HERO_CONTRAST,
                "contraste de {color:?} contra branco abaixo de {MIN_HERO_CONTRAST}"
            );
        }
        assert_ne!(from, to, "gradiente precisa de duas cores distintas");
        // `from` puxa para o vermelho, `to` para o azul.
        assert!(from.r() > from.b());
        assert!(to.b() > to.r());
    }

    /// `darken_for_contrast` escurece até o alvo e reduz luminância.
    #[test]
    fn darkening_reaches_contrast_target() {
        let gray = Color32::from_rgb(128, 128, 128);
        let darkened = darken_for_contrast(gray, MIN_HERO_CONTRAST);
        assert!(relative_luminance(darkened) <= relative_luminance(gray));
        assert!(contrast_ratio(darkened, Color32::WHITE) >= MIN_HERO_CONTRAST);
        // Uma cor já escura não deve ser escurecida além do necessário.
        let black = Color32::BLACK;
        assert!(
            contrast_ratio(
                darken_for_contrast(black, MIN_HERO_CONTRAST),
                Color32::WHITE
            ) >= 10.0
        );
    }

    /// Contraste WCAG sanity: preto vs branco = 21:1, cor igual = 1:1.
    #[test]
    fn contrast_ratio_basics() {
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.1);
        assert!((contrast_ratio(Color32::GRAY, Color32::GRAY) - 1.0).abs() < 1e-4);
    }

    /// O blur em software suaviza: desvio-padrão de luminância diminui.
    #[test]
    fn blur_reduces_luminance_variance() {
        let original = checkerboard_image(64, 8);
        let blurred = blurred_hero_background(&original);
        assert!(
            stddev_luma(&blurred) < stddev_luma(&original) * 0.5,
            "blur não suavizou o bastante"
        );
    }

    /// Capas sintéticas são determinísticas por seed e distintas entre seeds.
    #[test]
    fn sample_artwork_is_deterministic() {
        let a1 = sample_artwork(7, 32).to_rgba8().into_raw();
        let a2 = sample_artwork(7, 32).to_rgba8().into_raw();
        let b = sample_artwork(8, 32).to_rgba8().into_raw();
        assert_eq!(a1, a2, "mesma seed deve dar mesma capa");
        assert_ne!(a1, b, "seeds diferentes devem dar capas distintas");
        assert_eq!(a1.len(), 32 * 32 * 4);
    }

    // Helpers ---------------------------------------------------------------

    /// Retângulo com 3/4 esquerda `left` e 1/4 direita `right` (dominância explícita).
    fn two_tone_weighted(left: Color32, right: Color32, size: u32) -> DynamicImage {
        let mut img = RgbaImage::new(size, size);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let c = if x < size * 3 / 4 { left } else { right };
            *px = image::Rgba([c.r(), c.g(), c.b(), 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    /// Tabuleiro de quadrados claro/escuro.
    fn checkerboard_image(size: u32, cell: u32) -> DynamicImage {
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let on = ((x / cell) + (y / cell)).is_multiple_of(2);
            let v = if on { 255 } else { 0 };
            *px = image::Rgba([v, v, v, 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    /// Desvio-padrão da luminância média (0..255) de uma imagem.
    fn stddev_luma(img: &DynamicImage) -> f32 {
        let img = img.to_rgb8();
        let mean = img
            .pixels()
            .map(|p| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) as f32)
            .sum::<f32>()
            / (img.pixels().count() * 3) as f32;
        let variance = img
            .pixels()
            .map(|p| {
                let l = (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) as f32 / 3.0;
                (l - mean).powi(2)
            })
            .sum::<f32>()
            / img.pixels().count() as f32;
        variance.sqrt()
    }
}
