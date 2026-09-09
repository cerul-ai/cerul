# Security policy

## Supported versions

Security fixes target the latest code on the default branch. Use the latest
release when reporting a problem and include the affected version or commit.
Older releases do not have a separate maintenance branch.

## Report a vulnerability

Email [security@cerul.ai](mailto:security@cerul.ai), the reporting address listed
on the [Cerul security page](https://cerul.ai/security). Please do not disclose
security-sensitive details in public issues.

Include the affected version, reproduction steps, expected and actual behavior,
and potential impact. Prefer a small synthetic fixture. Remove credentials,
customer media, and unrelated personal data from attachments.

Maintainers will triage the report, investigate reproducible issues, and
coordinate a fix and disclosure with the reporter.

For general support or community conduct reports, contact
[support@cerul.ai](mailto:support@cerul.ai).

## Scope

This repository covers the local Rust core and CLI, embedded OCR, sidecars,
endpoint clients, and bundled media tools. Identify the affected component when
reporting a problem. Feature requests and non-security bugs belong in
[GitHub issues](https://github.com/cerul-ai/cerul/issues).
