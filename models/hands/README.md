# Embedded human-hand models

Source: https://github.com/opencv/opencv_zoo/tree/47534e27c9851bb1128ccc0102f1145e27f23f98/models

These unmodified FP32 ONNX conversions of MediaPipe models are published by
OpenCV Zoo under Apache-2.0 (see LICENSE). They are not the current MediaPipe
Tasks runtime or a promise of equivalent tracking behavior. Cerul implements
preprocessing, NMS, coordinate restoration and temporal association in Rust.

| File | Upstream directory and filename | SHA-256 |
| --- | --- | --- |
| palm.onnx | palm_detection_mediapipe/palm_detection_mediapipe_2023feb.onnx | 78ff51c38496b7fc8b8ebdb6cc8c1abb02fa6c38427c6848254cdaba57fcce7c |
| landmarks.onnx | handpose_estimation_mediapipe/handpose_estimation_mediapipe_2023feb.onnx | db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c |

Pre/postprocessing reference: mp_palmdet.py and mp_handpose.py in those same
directories, Copyright OpenCV Zoo contributors, Apache-2.0. The float models
are used because upstream warns of reduced accuracy in the int8 hand model.

Inputs are RGB NHWC float32 in [0,1]. Palm input is 192x192; hand input is
224x224. Output keypoints follow the MediaPipe 21-joint ordering. Cerul exports
image-space coordinates only; model hand-centered 3D output is not exported
as a calibrated world or robot pose.
