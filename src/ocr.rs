//! CPU OCR using embedded, provenance-pinned PP-OCRv6 small models.
use anyhow::{Result, ensure};
use image::{
    GrayImage, Luma, RgbImage,
    imageops::{self, FilterType},
};
use imageproc::contours::{BorderType, find_contours};
use imageproc::{
    geometric_transformations::{Interpolation, Projection, warp_into},
    geometry::min_area_rect,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, io::Cursor};
use tract_onnx::prelude::*;

type Plan = TypedRunnableModel<TypedModel>;
const DET: &[u8] = include_bytes!("../models/det.onnx");
const REC: &[u8] = include_bytes!("../models/rec.onnx");
pub const MIN_TEXT_CONFIDENCE: f32 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextBox {
    pub text: String,
    pub confidence: f32,
    pub xyxy: [f32; 4],
}

/// Keep one detector shape and a bounded LRU of recognition shapes.
#[derive(Default)]
pub struct Ocr {
    detector: Option<((u32, u32), Plan)>,
    recognizers: VecDeque<(u32, Plan)>,
}

fn compile(bytes: &[u8], height: u32, width: u32) -> Result<Plan> {
    crate::media::check_cancellation()?;
    let framework = tract_onnx::onnx().with_ignore_output_shapes(true);
    let mut proto = framework.proto_model_for_read(&mut Cursor::new(bytes))?;
    proto
        .graph
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("OCR model has no graph"))?
        .value_info
        .clear();
    framework
        .model_for_proto_model(&proto)?
        .with_input_fact(
            0,
            InferenceFact::dt_shape(f32::datum_type(), [1, 3, height as usize, width as usize]),
        )?
        .into_optimized()?
        .into_runnable()
}

fn tensor(image: &RgbImage, recognition: bool) -> Result<Tensor> {
    let (width, height) = image.dimensions();
    let mut tensor = Tensor::zero::<f32>(&[1, 3, height as usize, width as usize])?;
    let values = tensor.as_slice_mut::<f32>()?;
    for c in 0..3 {
        for y in 0..height {
            crate::media::check_cancellation()?;
            for x in 0..width {
                let pixel = image.get_pixel(x, y)[2 - c] as f32 / 255.;
                values[(c * height as usize + y as usize) * width as usize + x as usize] =
                    if recognition {
                        (pixel - 0.5) / 0.5
                    } else {
                        (pixel - [0.485, 0.456, 0.406][c]) / [0.229, 0.224, 0.225][c]
                    };
            }
        }
    }
    Ok(tensor)
}

impl Ocr {
    pub fn recognize_line(&mut self, image: &RgbImage) -> Result<(String, f32)> {
        crate::media::check_cancellation()?;
        ensure!(image.width() > 0 && image.height() > 0, "empty OCR image");
        let content_width = (48. * image.width() as f64 / image.height() as f64)
            .ceil()
            .clamp(1., 3200.) as u32;
        let width = content_width.max(320).div_ceil(32) * 32;
        if let Some(index) = self
            .recognizers
            .iter()
            .position(|(cached, _)| *cached == width)
        {
            let cached = self.recognizers.remove(index).unwrap();
            self.recognizers.push_back(cached);
        } else {
            if self.recognizers.len() == 4 {
                self.recognizers.pop_front();
            }
            self.recognizers
                .push_back((width, compile(REC, 48, width)?));
        }
        let resized = imageops::resize(image, content_width, 48, FilterType::Triangle);
        let mut padded = RgbImage::from_pixel(width, 48, image::Rgb([127, 127, 127]));
        imageops::replace(&mut padded, &resized, 0, 0);
        let outputs = self
            .recognizers
            .back()
            .unwrap()
            .1
            .run(tvec!(tensor(&padded, true)?.into()))?;
        crate::media::check_cancellation()?;
        let output = outputs[0].to_array_view::<f32>()?;
        let mut dictionary = vec![""];
        dictionary.extend(include_str!("../models/characters.txt").split('\n'));
        ensure!(
            output.ndim() == 3 && output.shape()[2] == dictionary.len(),
            "invalid OCR character output"
        );
        let mut text = String::new();
        let mut previous = 0;
        let mut confidence = 0.;
        let mut count = 0;
        for t in 0..output.shape()[1] {
            let best = (0..dictionary.len())
                .max_by(|&a, &b| output[[0, t, a]].total_cmp(&output[[0, t, b]]))
                .unwrap();
            if best != 0 && best != previous {
                text.push_str(dictionary[best]);
                confidence += output[[0, t, best]];
                count += 1;
            }
            previous = best;
        }
        Ok((
            text,
            if count == 0 {
                0.
            } else {
                confidence / count as f32
            },
        ))
    }

    pub fn read(&mut self, source: &RgbImage) -> Result<Vec<TextBox>> {
        crate::media::check_cancellation()?;
        ensure!(source.width() > 0 && source.height() > 0, "empty OCR image");
        let scale = (1080. / source.width().max(source.height()) as f64).min(1.);
        let rounded = |size: u32| ((size as f64 * scale / 32.).round() as u32).max(1) * 32;
        let (width, height) = (rounded(source.width()), rounded(source.height()));
        if self
            .detector
            .as_ref()
            .is_none_or(|(cached, _)| *cached != (width, height))
        {
            self.detector = Some(((width, height), compile(DET, height, width)?));
        }
        let resized = imageops::resize(source, width, height, FilterType::Triangle);
        let outputs = self
            .detector
            .as_ref()
            .unwrap()
            .1
            .run(tvec!(tensor(&resized, false)?.into()))?;
        let probabilities = outputs[0].to_array_view::<f32>()?;
        ensure!(
            probabilities.shape() == [1, 1, height as usize, width as usize],
            "invalid OCR detection output"
        );
        let bitmap = GrayImage::from_fn(width, height, |x, y| {
            Luma([if probabilities[[0, 0, y as usize, x as usize]] > 0.2 {
                255
            } else {
                0
            }])
        });
        let mut boxes = Vec::new();
        for contour in find_contours::<i32>(&bitmap).into_iter().take(3000) {
            crate::media::check_cancellation()?;
            if contour.border_type != BorderType::Outer || contour.points.len() < 4 {
                continue;
            }
            let points = &contour.points;
            let x0 = points.iter().map(|p| p.x).min().unwrap() as u32;
            let x1 = points.iter().map(|p| p.x).max().unwrap() as u32;
            let y0 = points.iter().map(|p| p.y).min().unwrap() as u32;
            let y1 = points.iter().map(|p| p.y).max().unwrap() as u32;
            if x1 - x0 < 3 || y1 - y0 < 3 {
                continue;
            }
            let mut area = 0f64;
            let mut perimeter = 0f64;
            for i in 0..points.len() {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                area += f64::from(a.x) * f64::from(b.y) - f64::from(b.x) * f64::from(a.y);
                perimeter += (f64::from(a.x - b.x).powi(2) + f64::from(a.y - b.y).powi(2)).sqrt();
            }
            let mut mask = GrayImage::new(x1 - x0 + 1, y1 - y0 + 1);
            let polygon: Vec<_> = points
                .iter()
                .map(|p| imageproc::point::Point::new(p.x - x0 as i32, p.y - y0 as i32))
                .collect();
            imageproc::drawing::draw_polygon_mut(&mut mask, &polygon, Luma([255]));
            let mut score = 0.;
            let mut count = 0;
            for (x, y, pixel) in mask.enumerate_pixels() {
                if pixel[0] > 0 {
                    score += probabilities[[0, 0, (y + y0) as usize, (x + x0) as usize]];
                    count += 1;
                }
            }
            if count == 0 || score / (count as f32) < 0.45 {
                continue;
            }
            let rect = min_area_rect(points).map(|p| (p.x as f32, p.y as f32));
            let length = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
            let (w, h) = (length(rect[0], rect[1]), length(rect[0], rect[3]));
            if w < 3. || h < 3. {
                continue;
            }
            let expand = (area.abs() * 0.5 * 1.4 / perimeter.max(1.)) as f32;
            let center = (
                rect.iter().map(|p| p.0).sum::<f32>() / 4.,
                rect.iter().map(|p| p.1).sum::<f32>() / 4.,
            );
            // Expand along the rotated rectangle's own axes, then map to source pixels.
            let u = ((rect[1].0 - rect[0].0) / w, (rect[1].1 - rect[0].1) / w);
            let v = ((rect[3].0 - rect[0].0) / h, (rect[3].1 - rect[0].1) / h);
            let expanded = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)].map(|(a, b)| {
                (
                    ((center.0 + a * (w / 2. + expand) * u.0 + b * (h / 2. + expand) * v.0)
                        * source.width() as f32
                        / width as f32)
                        .clamp(0., (source.width() - 1) as f32),
                    ((center.1 + a * (w / 2. + expand) * u.1 + b * (h / 2. + expand) * v.1)
                        * source.height() as f32
                        / height as f32)
                        .clamp(0., (source.height() - 1) as f32),
                )
            });
            boxes.push(expanded);
        }
        boxes.sort_by(|a, b| a[0].1.total_cmp(&b[0].1).then(a[0].0.total_cmp(&b[0].0)));
        let mut result = Vec::new();
        for corners in boxes {
            crate::media::check_cancellation()?;
            let length = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
            let w = length(corners[0], corners[1])
                .max(length(corners[3], corners[2]))
                .ceil() as u32;
            let h = length(corners[0], corners[3])
                .max(length(corners[1], corners[2]))
                .ceil() as u32;
            if w < 2 || h < 2 {
                continue;
            }
            let target = [
                (0., 0.),
                ((w - 1) as f32, 0.),
                ((w - 1) as f32, (h - 1) as f32),
                (0., (h - 1) as f32),
            ];
            let Some(projection) = Projection::from_control_points(corners, target) else {
                continue;
            };
            let mut crop = RgbImage::new(w, h);
            warp_into(
                source,
                &projection,
                Interpolation::Bilinear,
                *source.get_pixel(0, 0),
                &mut crop,
            );
            let vertical = h as f32 / w as f32 >= 1.5;
            if vertical {
                crop = imageops::rotate270(&crop);
            }
            let (mut text, mut confidence) = self.recognize_line(&crop)?;
            if vertical {
                let alternative = self.recognize_line(&imageops::rotate180(&crop))?;
                if alternative.1 > confidence {
                    (text, confidence) = alternative;
                }
            }
            let x0 = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let y0 = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let x1 = corners.iter().map(|p| p.0).fold(0., f32::max);
            let y1 = corners.iter().map(|p| p.1).fold(0., f32::max);
            if !text.trim().is_empty() && confidence >= MIN_TEXT_CONFIDENCE {
                result.push(TextBox {
                    text,
                    confidence,
                    xyxy: [
                        x0 / source.width() as f32,
                        y0 / source.height() as f32,
                        x1 / source.width() as f32,
                        y1 / source.height() as f32,
                    ],
                });
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_models_detect_and_read_real_pixels() {
        let image = image::load_from_memory(include_bytes!("../tests/fixtures/ocr-text.png"))
            .unwrap()
            .to_rgb8();
        let boxes = Ocr::default().read(&image).unwrap();
        let text = boxes
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(text.replace(' ', ""), "CERULVIDEO123");
        assert!(
            boxes
                .iter()
                .all(|b| b.xyxy.iter().all(|v| (0.0..=1.0).contains(v)))
        );
    }

    #[test]
    fn rotated_and_vertical_text_are_rectified_before_recognition() {
        let source = image::load_from_memory(include_bytes!("../tests/fixtures/ocr-text.png"))
            .unwrap()
            .to_rgb8();
        let mut canvas = RgbImage::from_pixel(768, 320, image::Rgb([255, 255, 255]));
        imageops::replace(&mut canvas, &source, 64, 112);
        let angled = imageproc::geometric_transformations::rotate_about_center(
            &canvas,
            12f32.to_radians(),
            Interpolation::Bilinear,
            image::Rgb([255, 255, 255]),
        );
        let mut engine = Ocr::default();
        for image in [angled, imageops::rotate90(&source)] {
            let boxes = engine.read(&image).unwrap();
            let text = boxes.iter().map(|b| b.text.as_str()).collect::<String>();
            assert_eq!(text.replace(' ', ""), "CERULVIDEO123");
            assert!(
                boxes
                    .iter()
                    .all(|b| b.xyxy.iter().all(|v| (0.0..=1.0).contains(v)))
            );
        }
    }
}
