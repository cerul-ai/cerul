# Test fixtures

`ocr-text.png` is an original synthetic image created for this repository:
black system-font text `CERUL VIDEO 123` on a white background. It is distributed
under the repository Apache-2.0 license and contains no customer data.

`hand-opencv.png` is a crop (490x366+0+25) of the first frame of OpenCV Zoo's
`models/handpose_estimation_mediapipe/example_outputs/mphandpose_demo.webp`
at revision `47534e27c9851bb1128ccc0102f1145e27f23f98`. The upstream directory explicitly licenses all
files under Apache-2.0; see `models/hands/LICENSE`. It depicts the upstream
public hand demo, including pre-existing annotation marks, not customer data.
This fixture tests real model inference and geometry; it is not an accuracy benchmark.

Source: https://github.com/opencv/opencv_zoo/blob/47534e27c9851bb1128ccc0102f1145e27f23f98/models/handpose_estimation_mediapipe/example_outputs/mphandpose_demo.webp

Derived PNG SHA-256: `d33ae8fefbead621d5938e435cee2bcadc911fb9a4f29f2ddeee3d717fce9eff`.
