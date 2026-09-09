"""Behavioral checks for public-doc and source-package guardrails."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "check_docs", Path(__file__).resolve().parents[1] / "scripts/check-docs.py"
)
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)


class DocumentationChecks(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        previous = docs.ROOT
        docs.ROOT = Path(self.directory.name).resolve()
        self.addCleanup(setattr, docs, "ROOT", previous)

    def write(self, path, text):
        target = docs.ROOT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        return target

    def test_moved_document_links_and_anchors(self):
        self.write("README.md", "[Guide](docs/guide.md#first-run)\n")
        self.write("docs/guide.md", "# First run\n[Home](../README.md)\n")
        self.assertEqual(docs.check()[2], [])
        self.write("docs/guide.md", "# Renamed\n[Gone](missing.md)\n")
        errors = docs.check()[2]
        self.assertTrue(any("missing anchor" in error for error in errors))
        self.assertTrue(any("broken local link" in error for error in errors))

    def test_shell_examples_are_extracted_without_execution(self):
        path = self.write(
            "README.md",
            """# Example
```sh
cerul annotate ./dataset --semantic subtask \\
  --write-lerobot --out ./copy # a comment
cerul --json search "a cup" > hits.json 2> events.jsonl
```
```text
cerul this-is-not-a-command-example
```
Use `cerul --yes remove ./demo.mp4` to remove its sidecars.
""",
        )
        examples = list(docs.commands(path, docs.split_document(path)[1]))
        self.assertEqual(len(examples), 3)
        self.assertEqual(
            examples[0]["args"],
            [
                "cerul",
                "annotate",
                "./dataset",
                "--semantic",
                "subtask",
                "--write-lerobot",
                "--out",
                "./copy",
            ],
        )
        self.assertEqual(examples[1]["args"], ["cerul", "--json", "search", "a cup"])
        self.assertEqual(
            examples[2]["args"], ["cerul", "--yes", "remove", "./demo.mp4"]
        )

    def test_package_must_contain_linked_assets_and_nested_guides(self):
        self.write(
            "README.md",
            "[Guide](docs/development/building.md)\n![logo](docs/assets/logo.svg)\n",
        )
        self.write("docs/development/building.md", "# Building\n")
        self.write("docs/assets/logo.svg", "<svg/>")
        inventory = self.write(
            "inventory.txt",
            "README.md\nLICENSE\nTHIRD_PARTY_NOTICES.md\nmodels/LICENSE\nscripts/check-docs.py\n",
        )
        errors = docs.check(inventory)[2]
        self.assertTrue(
            any(
                "missing public file: docs/development/building.md" in error
                for error in errors
            )
        )
        self.assertTrue(
            any(
                "link target absent from package: docs/assets/logo.svg" in error
                for error in errors
            )
        )

    def test_private_content_reports_location_without_echoing_value(self):
        secret = "ghp_" + "x" * 36
        self.write(
            "docs/guide.md", f"[Example]({secret})\n/home/example/private/video.mp4\n"
        )
        errors = docs.check()[2]
        self.assertEqual(len(errors), 2)
        self.assertTrue(all(secret not in error for error in errors))

    def test_package_rejects_private_artifacts_and_unreviewed_media(self):
        inventory = self.write(
            "inventory.txt",
            ".workspace/review.md\ndocs/customer.mp4\ndocs/customer.png\n",
        )
        errors = docs.check(inventory)[2]
        self.assertTrue(any("private/generated path" in error for error in errors))
        self.assertTrue(any("unexpected media" in error for error in errors))
        self.assertTrue(any("provenance review" in error for error in errors))

    def test_unclosed_fence_is_an_error(self):
        self.write("README.md", "```sh\ncerul status\n")
        self.assertIn("unclosed code fence", docs.check()[2][0])


if __name__ == "__main__":
    unittest.main()
