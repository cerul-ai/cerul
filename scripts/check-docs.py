#!/usr/bin/env python3
"""Check public Markdown and package contents without network access.

--commands emits direct Cerul invocations from shell fences and inline code for the CLI's
parser test. It never evaluates shell code or executes example commands.
"""

import argparse
import json
from pathlib import Path
import re
import shlex
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
LINK = re.compile(r'!?\[[^\]\n]*\]\(([^\s)]+)(?:\s+"[^"]*")?\)')
HTML_LINK = re.compile(r'(?:href|src|srcset)="([^"\s]+)"')
PRIVATE_PATH = re.compile(r"/(?:Users|home)/[A-Za-z0-9_.-]+/")
TOKEN = re.compile(
    r"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,}|AIza[A-Za-z0-9_-]{35}|AKIA[A-Z0-9]{16}|sk-(?:proj-|ant-)?[A-Za-z0-9_-]{40,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)"
)


def documents():
    paths = set(ROOT.glob("*.md"))
    for directory in (
        "docs",
        "examples",
        "models",
        "prompts",
        "skills",
        "tests/fixtures",
        ".github",
    ):
        paths.update((ROOT / directory).rglob("*.md"))
    return sorted(paths)


def split_document(path):
    prose, shell = [], []
    fence, language, start = None, "", 0
    for number, line in enumerate(path.read_text().splitlines(), 1):
        match = FENCE.match(line)
        if match and fence is None:
            fence, language, start = match[1], match[2].strip(), number
        elif (
            match
            and match[1][0] == fence[0]
            and len(match[1]) >= len(fence)
            and not match[2].strip()
        ):
            fence = None
        elif fence is None:
            prose.append((number, line))
        elif language in ("sh", "bash", "shell", "zsh"):
            shell.append((number, line))
    if fence is not None:
        raise ValueError(f"{path.relative_to(ROOT)}:{start}: unclosed code fence")
    # Inline examples use complete commands too; placeholders belong in prose.
    for number, line in prose:
        shell.extend(
            (number, command) for command in re.findall(r"`(cerul(?: [^`]+)?)`", line)
        )
    return prose, shell


def anchors(prose):
    result, counts = set(), {}
    for _, line in prose:
        match = re.match(r"^\s{0,3}#{1,6}\s+(.+?)(?:\s+#+)?$", line)
        if match:
            heading = re.sub(r"<[^>]+>", "", match[1]).lower()
            slug = re.sub(r"[^\w\- ]", "", heading).replace(" ", "-")
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            result.add(f"{slug}-{count}" if count else slug)
        result.update(re.findall(r'(?:id|name)="([^"]+)"', line))
    return result


def commands(path, shell):
    pending, start = "", 0
    for number, line in shell:
        if not pending:
            start = number
        pending += line.strip()
        if pending.endswith("\\"):
            pending = pending[:-1] + " "
            continue
        text, pending = pending, ""
        if not re.match(r"^cerul(?:\s|$)", text):
            continue
        lexer = shlex.shlex(text, posix=True, punctuation_chars="|&;<>")
        lexer.whitespace_split = True
        args = list(lexer)
        for index, arg in enumerate(args):
            if arg in ("|", "||", "&", "&&", ";", ">", ">>", "<"):
                args = args[:index]
                break
        yield {"path": str(path.relative_to(ROOT)), "line": start, "args": args}


def check(package_list=None):
    errors = []
    paths = documents()
    parsed = {}
    for path in paths:
        try:
            parsed[path] = split_document(path)
        except ValueError as error:
            errors.append(str(error))
        for number, line in enumerate(path.read_text().splitlines(), 1):
            for name, pattern in (
                ("personal absolute path", PRIVATE_PATH),
                ("credential-like content", TOKEN),
            ):
                if pattern.search(line):
                    errors.append(f"{path.relative_to(ROOT)}:{number}: {name}")
    inventory = None
    if package_list:
        inventory = set(Path(package_list).read_text().splitlines())
        for name in inventory:
            parts = Path(name).parts
            if any(
                part
                in (
                    ".workspace",
                    ".env",
                    "target",
                    ".git",
                    "node_modules",
                    ".cerul",
                    ".codex",
                    ".claude",
                    "artifacts",
                    "test-results",
                )
                or part.startswith(".env.")
                for part in parts
            ):
                errors.append(f"package: unexpected private/generated path: {name}")
            if Path(name).suffix.lower() in (
                ".mp4",
                ".mov",
                ".mkv",
                ".avi",
                ".wav",
                ".mp3",
                ".parquet",
                ".lance",
                ".jsonl",
                ".pem",
                ".key",
            ):
                errors.append(
                    f"package: unexpected media, data, or key material: {name}"
                )
            if Path(name).suffix.lower() in (
                ".png",
                ".jpg",
                ".jpeg",
                ".onnx",
            ) and name not in {
                "docs/assets/cerul-logo-dark.png",
                "docs/assets/cerul-logo-light.png",
                "docs/assets/cerul-architecture.png",
                "tests/fixtures/ocr-text.png",
                "models/det.onnx",
                "models/rec.onnx",
            }:
                errors.append(
                    f"package: asset needs explicit provenance review: {name}"
                )
        required = {
            str(path.relative_to(ROOT))
            for path in paths
            if path.name != "AGENTS.md" and path.relative_to(ROOT).parts[0] != ".github"
        }
        required.update(
            {
                "LICENSE",
                "THIRD_PARTY_NOTICES.md",
                "models/LICENSE",
                "scripts/check-docs.py",
            }
        )
        for name in sorted(required - inventory):
            errors.append(f"package: missing public file: {name}")
    for path, (prose, _) in parsed.items():
        for number, line in prose:
            for destination in LINK.findall(line) + HTML_LINK.findall(line):
                if TOKEN.search(destination) or PRIVATE_PATH.search(destination):
                    continue  # Content checks report the location without echoing it.
                url = urlsplit(destination.strip("<>"))
                if url.scheme or url.netloc:
                    continue
                target = (
                    (path.parent / unquote(url.path)).resolve() if url.path else path
                )
                location = f"{path.relative_to(ROOT)}:{number}"
                if not target.is_relative_to(ROOT) or not target.exists():
                    errors.append(f"{location}: broken local link: {destination}")
                    continue
                if url.fragment and target.suffix == ".md" and target in parsed:
                    if unquote(url.fragment) not in anchors(parsed[target][0]):
                        errors.append(f"{location}: missing anchor: {destination}")
                if inventory is not None and str(path.relative_to(ROOT)) in inventory:
                    name = str(target.relative_to(ROOT))
                    if name not in inventory and not any(
                        item.startswith(name + "/") for item in inventory
                    ):
                        errors.append(
                            f"{location}: link target absent from package: {destination}"
                        )
    return paths, parsed, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commands", action="store_true")
    parser.add_argument("--package-list", type=Path)
    args = parser.parse_args()
    paths, parsed, errors = check(args.package_list)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    examples = [
        command
        for path, (_, shell) in parsed.items()
        for command in commands(path, shell)
    ]
    if args.commands:
        print(json.dumps(examples))
    else:
        print(
            f"Checked {len(paths)} public Markdown files and extracted {len(examples)} Cerul command examples."
        )
        if args.package_list:
            print("Source-package documentation and content checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
