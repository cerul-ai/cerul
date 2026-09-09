# Embedded OCR assets

Cerul embeds the unmodified Apache-2.0 PP-OCRv6 small ONNX weights published by
PaddlePaddle. No weights are downloaded at runtime.

| File | Official repository | Revision | SHA-256 |
| --- | --- | --- | --- |
| det.onnx | https://huggingface.co/PaddlePaddle/PP-OCRv6_small_det_onnx | 28fe5895c24fd108c19eb3e8479f4ab385fbfc62 | d73e0058b7a8086bbd57f3d10b8bcd4ff95363f67e06e2762b5e814fe9c9410e |
| rec.onnx | https://huggingface.co/PaddlePaddle/PP-OCRv6_small_rec_onnx | b8f84f0b80c529de40b4fbb3544b84fa7233a513 | 5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634 |

`characters.txt` is extracted in original order from the recognizer's
`inference.yml` at the pinned revision, followed by the space character used by
CTCLabelDecode. Blank is class zero and is not included in this file.

The upstream exported intermediate shape annotations conflict with concrete
input dimensions in tract. Cerul removes those annotations in memory and lets
tract infer shapes from the unmodified graph. This does not modify any weights.

Copyright PaddlePaddle Authors. See LICENSE for Apache License 2.0.
