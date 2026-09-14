//! CPU MediaPipe ONNX inference. See models/hands/README.md for provenance.
//! Geometry follows the Apache-2.0 OpenCV Zoo reference; no OpenCV runtime.
use anyhow::{Result, ensure};
use image::RgbImage;
use std::{io::Cursor, sync::Arc};
use tract_onnx::prelude::*;

pub const PALM_HASH: &str = "78ff51c38496b7fc8b8ebdb6cc8c1abb02fa6c38427c6848254cdaba57fcce7c";
pub const HAND_HASH: &str = "db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c";
pub struct Detector {
    palm: Arc<TypedRunnableModel>,
    hand: Arc<TypedRunnableModel>,
}
#[derive(Clone)]
struct Palm {
    bbox: [f32; 4],
    points: [[f32; 2]; 7],
    score: f32,
}
fn compile(bytes: &[u8]) -> Result<Arc<TypedRunnableModel>> {
    crate::media::check_cancellation()?;
    tract_onnx::onnx()
        .model_for_read(&mut Cursor::new(bytes))?
        .into_optimized()?
        .into_runnable()
}
/// Sample in display coordinates, padding outside the source with black.
fn sample(image: &RgbImage, x: f32, y: f32, channel: usize) -> f32 {
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let dx = x - x.floor();
    let dy = y - y.floor();
    let mut value = 0.;
    for (ox, wx) in [(0, 1. - dx), (1, dx)] {
        for (oy, wy) in [(0, 1. - dy), (1, dy)] {
            let (px, py) = (x0 + ox, y0 + oy);
            if px >= 0 && py >= 0 && px < image.width() as i64 && py < image.height() as i64 {
                value += image.get_pixel(px as u32, py as u32)[channel] as f32 * wx * wy;
            }
        }
    }
    value / 255.
}
fn tensor(image: &RgbImage, size: usize, map: impl Fn(f32, f32) -> [f32; 2]) -> Result<Tensor> {
    let mut tensor = Tensor::zero::<f32>(&[1, size, size, 3])?;
    let mut plain = tensor.try_as_plain_mut()?;
    let values = plain.as_slice_mut::<f32>()?;
    for y in 0..size {
        crate::media::check_cancellation()?;
        for x in 0..size {
            let [sx, sy] = map(x as f32 + 0.5, y as f32 + 0.5);
            for c in 0..3 {
                values[(y * size + x) * 3 + c] = sample(image, sx - 0.5, sy - 0.5, c);
            }
        }
    }
    Ok(tensor)
}
fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let intersection =
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.);
    let area = |r: [f32; 4]| (r[2] - r[0]).max(0.) * (r[3] - r[1]).max(0.);
    intersection / (area(a) + area(b) - intersection).max(1e-6)
}
#[derive(Clone, Copy)]
struct Crop {
    center: [f32; 2],
    size: f32,
    cos: f32,
    sin: f32,
}
impl Crop {
    fn point(self, x: f32, y: f32) -> [f32; 2] {
        let x = (x / 224. - 0.5) * self.size;
        let y = (y / 224. - 0.5) * self.size;
        [
            self.center[0] + self.cos * x - self.sin * y,
            self.center[1] + self.sin * x + self.cos * y,
        ]
    }
}
fn crop(palm: &Palm) -> Crop {
    let wrist = palm.points[0];
    let middle = palm.points[2];
    let angle = std::f32::consts::FRAC_PI_2 - (-(middle[1] - wrist[1])).atan2(middle[0] - wrist[0]);
    let (sin, cos) = angle.sin_cos();
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for [x, y] in palm.points {
        let (rx, ry) = (cos * x + sin * y, -sin * x + cos * y);
        bounds[0] = bounds[0].min(rx);
        bounds[1] = bounds[1].min(ry);
        bounds[2] = bounds[2].max(rx);
        bounds[3] = bounds[3].max(ry);
    }
    let (w, h) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
    let cx = (bounds[0] + bounds[2]) / 2.;
    let cy = (bounds[1] + bounds[3]) / 2. - 0.4 * h;
    Crop {
        center: [cos * cx - sin * cy, sin * cx + cos * cy],
        size: (3. * w.max(h)).max(1.),
        cos,
        sin,
    }
}
impl Detector {
    pub fn new() -> Result<Self> {
        Ok(Self {
            palm: compile(include_bytes!("../../models/hands/palm.onnx"))?,
            hand: compile(include_bytes!("../../models/hands/landmarks.onnx"))?,
        })
    }
    pub fn infer(&self, image: &RgbImage) -> Result<Vec<super::hands::Hand>> {
        ensure!(image.width() > 0 && image.height() > 0, "empty hand image");
        let scale = image.width().max(image.height()) as f32 / 192.;
        let pad = [
            (192. - image.width() as f32 / scale) / 2.,
            (192. - image.height() as f32 / scale) / 2.,
        ];
        let output = self.palm.run(tvec!(
            tensor(image, 192, |x, y| [
                (x - pad[0]) * scale,
                (y - pad[1]) * scale
            ])?
            .into()
        ))?;
        crate::media::check_cancellation()?;
        let boxes = output[0].to_plain_array_view::<f32>()?;
        let scores = output[1].to_plain_array_view::<f32>()?;
        ensure!(
            boxes.shape() == [1, 2016, 18] && scores.shape() == [1, 2016, 1],
            "invalid palm model output"
        );
        let mut palms = Vec::new();
        let mut index = 0;
        for (grid, repeats) in [(24, 2), (12, 6)] {
            for y in 0..grid {
                for x in 0..grid {
                    for _ in 0..repeats {
                        let i = index;
                        index += 1;
                        let score = 1. / (1. + (-scores[[0, i, 0]]).exp());
                        if !score.is_finite() || score < 0.5 {
                            continue;
                        }
                        let anchor = [
                            (x as f32 + 0.5) * 192. / grid as f32,
                            (y as f32 + 0.5) * 192. / grid as f32,
                        ];
                        let point = |a: f32, b: f32| {
                            [
                                (a + anchor[0] - pad[0]) * scale,
                                (b + anchor[1] - pad[1]) * scale,
                            ]
                        };
                        let [cx, cy] = point(boxes[[0, i, 0]], boxes[[0, i, 1]]);
                        let (w, h) = (boxes[[0, i, 2]] * scale, boxes[[0, i, 3]] * scale);
                        let points = std::array::from_fn(|j| {
                            point(boxes[[0, i, 4 + j * 2]], boxes[[0, i, 5 + j * 2]])
                        });
                        if w > 0. && h > 0. && points.iter().flatten().all(|v| v.is_finite()) {
                            palms.push(Palm {
                                bbox: [cx - w / 2., cy - h / 2., cx + w / 2., cy + h / 2.],
                                points,
                                score,
                            });
                        }
                    }
                }
            }
        }
        palms.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut kept: Vec<Palm> = Vec::new();
        for palm in palms {
            if kept.iter().all(|other| iou(palm.bbox, other.bbox) < 0.3) {
                kept.push(palm);
                if kept.len() == 4 {
                    break;
                }
            }
        }
        let mut hands = Vec::new();
        for palm in kept {
            crate::media::check_cancellation()?;
            let crop = crop(&palm);
            let output = self
                .hand
                .run(tvec!(tensor(image, 224, |x, y| crop.point(x, y))?.into()))?;
            let landmarks = output[0].to_plain_array_view::<f32>()?;
            let confidence = output[1].to_plain_array_view::<f32>()?[[0, 0]];
            let right = output[2].to_plain_array_view::<f32>()?[[0, 0]];
            ensure!(landmarks.shape() == [1, 63], "invalid hand model output");
            if !confidence.is_finite() || confidence < 0.8 || !right.is_finite() {
                continue;
            }
            let keypoints = std::array::from_fn(|i| {
                let [x, y] = crop.point(landmarks[[0, i * 3]], landmarks[[0, i * 3 + 1]]);
                let p = [x / image.width() as f32, y / image.height() as f32];
                p.iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    .then_some(p)
            });
            hands.push(super::hands::Hand {
                track_id: 0,
                handedness: if right >= 0.5 {
                    super::hands::Side::Right
                } else {
                    super::hands::Side::Left
                },
                handedness_score: right.max(1. - right).clamp(0., 1.),
                confidence: confidence.clamp(0., 1.),
                keypoints,
            });
            if hands.len() == 2 {
                break;
            }
        }
        Ok(hands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_restores_rotated_center_and_nms_uses_box_extents() {
        let palm = Palm {
            bbox: [0., 0., 10., 10.],
            points: [
                [5., 10.],
                [0., 5.],
                [5., 0.],
                [10., 5.],
                [2., 4.],
                [4., 6.],
                [6., 6.],
            ],
            score: 1.,
        };
        let c = crop(&palm);
        assert_eq!(c.point(112., 112.), c.center);
        assert!((c.size - 30.).abs() < 0.01);
        assert_eq!(iou([10., 10., 20., 20.], [10., 10., 20., 20.]), 1.);
        assert_eq!(iou([10., 10., 20., 20.], [21., 10., 30., 20.]), 0.);
    }
}
