//! Caption and credit strips rendered to PNG by the CLI.
//!
//! Burned-in text must not depend on libass or libfreetype being present in
//! the ffmpeg build, so the CLI rasterises each strip itself with a bundled
//! OFL Latin font and, when a cue needs it, a CJK font found on the system.
use ab_glyph::{Font, FontRef, FontVec, Glyph, GlyphId, OutlinedGlyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result, ensure};
use image::{ImageEncoder, Rgba, RgbaImage};
use std::path::Path;

/// Inter Regular 4.001, SIL Open Font License 1.1; see packaging/licenses.
pub const LATIN_FONT: &[u8] = include_bytes!("../../packaging/fonts/Inter-Regular.otf");

/// System CJK fonts tried in order when the bundled font lacks a glyph.
/// `CERUL_CJK_FONT` names a file to try first.
pub const CJK_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

/// Whichever font supplies a glyph; `Font` is not object safe, so an enum
/// stands in for a trait object.
enum Face<'a> {
    Latin(&'a FontRef<'static>),
    Cjk(&'a FontVec),
}
impl Face<'_> {
    fn advance(&self, id: GlyphId, scale: PxScale) -> f32 {
        match self {
            Self::Latin(font) => font.as_scaled(scale).h_advance(id),
            Self::Cjk(font) => font.as_scaled(scale).h_advance(id),
        }
    }
    fn outline(&self, glyph: Glyph) -> Option<OutlinedGlyph> {
        match self {
            Self::Latin(font) => font.outline_glyph(glyph),
            Self::Cjk(font) => font.outline_glyph(glyph),
        }
    }
}

pub struct Fonts {
    latin: FontRef<'static>,
    cjk: Option<FontVec>,
    /// Set once a lookup ran and found nothing, so it is not repeated per strip.
    cjk_missing: bool,
    pub cjk_path: Option<String>,
}
impl Fonts {
    pub fn load() -> Result<Self> {
        Ok(Self {
            latin: FontRef::try_from_slice(LATIN_FONT).context("bundled caption font")?,
            cjk: None,
            cjk_missing: false,
            cjk_path: None,
        })
    }
    fn read_font(path: &Path) -> Option<FontVec> {
        let bytes = std::fs::read(path).ok()?;
        FontVec::try_from_vec_and_index(bytes, 0).ok()
    }
    /// Load a system CJK font the first time a character needs one. Without
    /// one, those characters render as the bundled font's missing-glyph box.
    pub fn prepare(&mut self, text: &str) {
        if self.cjk.is_some() || self.cjk_missing {
            return;
        }
        if !text.chars().any(|c| self.latin.glyph_id(c).0 == 0) {
            return;
        }
        let override_path = std::env::var("CERUL_CJK_FONT").ok();
        let candidates = override_path
            .iter()
            .map(String::as_str)
            .chain(CJK_CANDIDATES.iter().copied());
        for candidate in candidates {
            if let Some(font) = Self::read_font(Path::new(candidate)) {
                self.cjk_path = Some(candidate.to_owned());
                self.cjk = Some(font);
                return;
            }
        }
        self.cjk_missing = true;
    }
    fn glyph(&self, c: char) -> (Face<'_>, GlyphId) {
        let id = self.latin.glyph_id(c);
        if id.0 != 0 {
            return (Face::Latin(&self.latin), id);
        }
        if let Some(cjk) = &self.cjk {
            let fallback = cjk.glyph_id(c);
            if fallback.0 != 0 {
                return (Face::Cjk(cjk), fallback);
            }
        }
        (Face::Latin(&self.latin), id)
    }
    /// Does the bundled font or the loaded fallback have a glyph for `c`?
    pub fn covers(&self, c: char) -> bool {
        self.glyph(c).1.0 != 0
    }
    fn line_width(&self, line: &str, scale: PxScale) -> f32 {
        line.chars()
            .map(|c| {
                let (face, id) = self.glyph(c);
                face.advance(id, scale)
            })
            .sum()
    }
}

/// One rendered strip, full output width, transparent outside the text box.
pub struct Strip {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Caption size for an output frame: readable on a phone, never wider than
/// the frame for a 32-character line.
pub fn caption_px(width: u32, height: u32) -> f32 {
    (width as f32 / 18.).min(height as f32 / 22.).max(12.)
}

/// Render `lines` centred in a strip `width` pixels wide. Text is white on a
/// translucent black box; the font shrinks when a line would not fit.
pub fn strip(fonts: &Fonts, lines: &[String], width: u32, font_px: f32) -> Result<Strip> {
    ensure!(!lines.is_empty(), "nothing to render");
    ensure!(width >= 64, "strip too narrow");
    let mut font_px = font_px.max(8.);
    let limit = width as f32 * 0.92;
    let widest = |px: f32| {
        lines
            .iter()
            .map(|line| fonts.line_width(line, PxScale::from(px)))
            .fold(0f32, f32::max)
    };
    while widest(font_px) > limit && font_px > 8. {
        font_px *= 0.9;
    }
    let scale = PxScale::from(font_px);
    let padding = (font_px * 0.45).round();
    let line_height = (font_px * 1.3).round();
    let latin = fonts.latin.as_scaled(scale);
    let ascent = latin.ascent();
    let box_width = (widest(font_px) + 2. * padding).min(width as f32).round();
    let height = (line_height * lines.len() as f32 + 2. * padding).round() as u32;
    let mut image = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    let box_left = ((width as f32 - box_width) / 2.).round().max(0.) as u32;
    for y in 0..height {
        for x in box_left..(box_left + box_width as u32).min(width) {
            image.put_pixel(x, y, Rgba([0, 0, 0, 168]));
        }
    }
    for (index, line) in lines.iter().enumerate() {
        let line_width = fonts.line_width(line, scale);
        let mut x = (width as f32 - line_width) / 2.;
        let baseline = padding + line_height * index as f32 + (line_height - font_px) / 2. + ascent;
        for c in line.chars() {
            let (face, id) = fonts.glyph(c);
            let glyph: Glyph = id.with_scale_and_position(scale, point(x, baseline));
            if let Some(outline) = face.outline(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i64 + gx as i64;
                    let py = bounds.min.y as i64 + gy as i64;
                    if px < 0 || py < 0 || px >= width as i64 || py >= height as i64 {
                        return;
                    }
                    let pixel = image.get_pixel_mut(px as u32, py as u32);
                    let alpha = (coverage * 255.) as u32;
                    if alpha == 0 {
                        return;
                    }
                    // Source-over white onto whatever is there, in u32 so
                    // 255³ cannot overflow.
                    let inverse = 255 - alpha;
                    let below = u32::from(pixel[3]);
                    let out_alpha = alpha + below * inverse / 255;
                    for channel in 0..3 {
                        pixel[channel] = ((255 * alpha
                            + u32::from(pixel[channel]) * below * inverse / 255)
                            / out_alpha.max(1)) as u8;
                    }
                    pixel[3] = out_alpha.min(255) as u8;
                });
            }
            x += face.advance(id, scale);
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        image.as_raw(),
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(Strip { png, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_renders_latin_and_credit_punctuation_into_a_centred_box() {
        let fonts = Fonts::load().unwrap();
        for c in ['·', '–', 'é', '0', 'W'] {
            assert!(fonts.covers(c), "{c}");
        }
        assert!(!fonts.covers('中'));
        let strip = strip(
            &fonts,
            &["we cut the price".into(), "from 29 to 15".into()],
            640,
            24.,
        )
        .unwrap();
        assert_eq!(strip.width, 640);
        let image = image::load_from_memory(&strip.png).unwrap().to_rgba8();
        assert_eq!(image.height(), strip.height);
        assert_eq!(
            image.get_pixel(0, 0)[3],
            0,
            "outside the box is transparent"
        );
        let middle = image.get_pixel(320, strip.height / 2);
        assert!(middle[3] > 0, "the box is drawn");
        let white = image
            .pixels()
            .filter(|p| p[3] > 200 && p[0] > 200 && p[1] > 200 && p[2] > 200)
            .count();
        assert!(white > 100, "text was drawn: {white}");
        // Wide text shrinks instead of overflowing the strip.
        let wide = super::strip(&fonts, &["W".repeat(32)], 320, 40.).unwrap();
        assert_eq!(wide.width, 320);
        assert!(wide.height < strip.height * 2);
        assert!(caption_px(1920, 1080) < caption_px(1080, 1920));
    }

    #[test]
    fn cjk_falls_back_to_a_system_font_or_a_missing_glyph_box() {
        let mut fonts = Fonts::load().unwrap();
        fonts.prepare("价格");
        let strip = strip(&fonts, &["价格从29降到15".into()], 480, 24.).unwrap();
        assert!(!strip.png.is_empty());
        if fonts.cjk_path.is_some() {
            assert!(fonts.covers('价'));
        }
        // An absent override does not break Latin rendering.
        let mut missing = Fonts::load().unwrap();
        missing.cjk_missing = true;
        missing.prepare("中文");
        assert!(super::strip(&missing, &["中文 and latin".into()], 480, 24.).is_ok());
    }
}
