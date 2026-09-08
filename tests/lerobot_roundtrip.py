"""Acceptance fixture and official-loader checks; not a Cerul runtime dependency."""
import json
import sys
from pathlib import Path

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq
import torch
from lerobot.configs.video import RGBEncoderConfig
from lerobot.datasets.io_utils import write_table_one_row_group_per_episode
from lerobot.datasets.language import (
    language_events_arrow_type,
    language_feature_info,
    language_persistent_arrow_type,
)
from lerobot.datasets.lerobot_dataset import LeRobotDataset


def create(root, with_language=True):
    camera = "observation.images.front"
    features = {
        "action": {"dtype": "float32", "shape": (2,), "names": None},
        "observation.state": {"dtype": "float32", "shape": (2,), "names": None},
        camera: {"dtype": "video", "shape": (64, 64, 3), "names": ["height", "width", "channels"]},
    }
    dataset = LeRobotDataset.create(
        "cerul/acceptance", fps=2, features=features, root=root,
        robot_type="fixture", video_backend="pyav", image_writer_threads=2,
        rgb_encoder=RGBEncoderConfig(vcodec="h264"),
    )
    for episode in range(5):
        for frame in range(8):
            pixels = np.zeros((64, 64, 3), dtype=np.uint8)
            pixels[:, :, 0] = episode * 40
            pixels[:, :, 1] = frame * 25
            dataset.add_frame({
                "action": np.array([episode, frame], dtype=np.float32),
                "observation.state": np.array([frame + 0.5, episode], dtype=np.float32),
                camera: pixels, "task": "Move the cup",
            })
        dataset.save_episode(parallel_encoding=False)
    dataset.finalize()
    if with_language:
        add_language_fixture(root)
    else:
        mark_v31(root)


def language_array(rows, target):
    # PyArrow cannot construct nested extension values directly from Python.
    fields = [pa.field(f.name, pa.list_(pa.string()) if f.name == "tool_calls" else f.type,
                       nullable=f.nullable) for f in target.value_type]
    return pa.array(rows, type=pa.list_(pa.struct(fields))).cast(target)


def add_language_fixture(root):
    # The pinned official writer still emits v3.0. This is a synthetic v3.1
    # compatibility fixture using its language schema, not an official upgrade.
    for path in sorted((root / "data").rglob("*.parquet")):
        table = pq.read_table(path)
        atoms = [{"role": "assistant", "content": "Preserved plan", "style": "plan",
                  "timestamp": 0.0, "camera": None, "tool_calls": None},
                 {"role": "assistant", "content": "Original subtask", "style": "subtask",
                  "timestamp": 0.0, "camera": None, "tool_calls": None}]
        table = table.append_column("language_persistent", language_array([atoms] * table.num_rows, language_persistent_arrow_type()))
        table = table.append_column("language_events", language_array([[]] * table.num_rows, language_events_arrow_type()))
        write_table_one_row_group_per_episode(table, path)
    info_path = root / "meta/info.json"
    info = json.loads(info_path.read_text())
    info["codebase_version"] = "v3.1"
    info["features"].update(language_feature_info())
    info_path.write_text(json.dumps(info, indent=2) + "\n")
    print(json.dumps({"created": str(root), "episodes": 5, "frames": 40}))


def mark_v31(root):
    info_path = root / "meta/info.json"
    info = json.loads(info_path.read_text())
    info["codebase_version"] = "v3.1"
    info_path.write_text(json.dumps(info, indent=2) + "\n")
    print(json.dumps({"created": str(root), "episodes": 5, "frames": 40, "language": False}))


def verify(source, output):
    before = LeRobotDataset("cerul/acceptance", root=source, video_backend="pyav")
    after = LeRobotDataset("cerul/acceptance", root=output, video_backend="pyav")
    assert len(before) == len(after) == 40
    for index in range(40):
        old, new = before[index], after[index]
        for key, value in old.items():
            if key == "language_persistent":
                continue
            if isinstance(value, torch.Tensor):
                assert torch.equal(value, new[key]), (index, key)
            else:
                assert value == new[key], (index, key)
    # Compare actual stored atoms as well as loader tensors. The persistent column
    # broadcasts the episode timeline; its latest timestamp identifies the active atom.
    for path in sorted((source / "data").rglob("*.parquet")):
        old = pq.read_table(path).to_pylist()
        new = pq.read_table(output / path.relative_to(source)).to_pylist()
        assert len(old) == len(new)
        for a, b in zip(old, new):
            episode = a["episode_index"]
            for key in a:
                if key != "language_persistent":
                    assert a[key] == b[key], (episode, key)
            if episode % 2:
                assert a.get("language_persistent") == b["language_persistent"]
                continue
            assert [r for r in a.get("language_persistent", []) if r["style"] != "subtask"] == [r for r in b["language_persistent"] if r["style"] != "subtask"]
            active = max((r for r in b["language_persistent"] if r["style"] == "subtask" and r["timestamp"] <= b["timestamp"]), key=lambda r: r["timestamp"])
            assert active["content"] == ("First subtask" if b["timestamp"] < 2 else "Second subtask")
    print(json.dumps({"official_loader": "passed", "frames_checked": 40, "episodes_checked": 5}))


if __name__ == "__main__":
    if sys.argv[1] == "create":
        create(Path(sys.argv[2]), "--without-language" not in sys.argv[3:])
    elif sys.argv[1] == "add-language-fixture":
        add_language_fixture(Path(sys.argv[2]))
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        raise ValueError("expected create or verify")
