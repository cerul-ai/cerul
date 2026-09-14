//! Small built-in bitmap lettering for portable review-video captions.
//! Glyph patterns are maintained here; no system fonts or libass are required.
use image::{Rgb, RgbImage};

fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 0, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        ';' => [0, 12, 12, 0, 4, 4, 8],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '"' => [10, 10, 0, 0, 0, 0, 0],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        ' ' => [0; 7],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}

pub fn panel(width: u32, text: &str, watermark: bool) -> RgbImage {
    let scale = (width / 420).clamp(1, 5);
    let columns = ((width.saturating_sub(16)) / (6 * scale)).max(1) as usize;
    let mut chars = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<Vec<_>>();
    if chars.len() > columns * 3 {
        chars.truncate(columns * 3);
        for c in chars.iter_mut().rev().take(3) {
            *c = '.';
        }
    }
    let height = 16 + 4 * 10 * scale;
    let mut image = RgbImage::from_pixel(width, height, Rgb([18, 22, 29]));
    let mut draw = |line: usize, values: &[char], color: Rgb<u8>| {
        for (i, &c) in values.iter().enumerate() {
            for (row, bits) in glyph(c).into_iter().enumerate() {
                for col in 0..5u32 {
                    if bits & (1 << (4 - col)) != 0 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                let x = 8 + i as u32 * 6 * scale + col * scale + dx;
                                let y = 8 + line as u32 * 10 * scale + row as u32 * scale + dy;
                                if x < width && y < height {
                                    image.put_pixel(x, y, color);
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    for (line, values) in chars.chunks(columns).enumerate() {
        draw(line, values, Rgb([240, 244, 248]));
    }
    if watermark {
        draw(
            3,
            &"Annotated with Cerul"
                .chars()
                .take(columns)
                .collect::<Vec<_>>(),
            Rgb([98, 190, 208]),
        );
    }
    image
}
