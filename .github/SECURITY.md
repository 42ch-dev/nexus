# Security Policy

Nexus is a local-first, AI-driven narrative orchestration engine. This repository ([`42ch-dev/nexus`](https://github.com/42ch-dev/nexus)) contains the `nexus42` CLI with its integrated daemon, JSON Schema wire contracts, and the published `@42ch/nexus-contracts` npm package. This policy covers security issues in that code and in the artifacts built from it.

## Supported versions

Nexus is **pre-1.0**: breaking changes are expected, and there are no LTS branches, backports, or guaranteed response or fix timelines. Security fixes land on `main` and, when applicable, in the next published release.

| Version | Supported |
| --- | --- |
| Latest `main` | Yes |
| Latest published release | Yes |
| Older releases, tags, and forks | Fixes target `main` and the next release |

If practical, include in your report whether the issue reproduces on the latest `main`. Reports about older versions are still welcome when the issue affects current code or published artifacts.

## Reporting a vulnerability

**Do not open a public issue or pull request for a suspected vulnerability, and do not publish exploit details before a fix is released.**

Report privately through GitHub Private Vulnerability Reporting:

1. Open <https://github.com/42ch-dev/nexus/security/advisories/new>, or use the repository **Security** tab → **Report a vulnerability**.
2. Submit the details below. The draft advisory is visible only to you and the maintainers until it is published.

A GitHub account is required, and this form is the private reporting channel for this repository.

### What to include

- **Component and version** — crate, npm package, or binary name (`nexus42`, `@42ch/nexus-contracts`, …) plus the version, tag, or commit.
- **Environment** — operating system, architecture, install method (source, npm, or release binary), and any relevant configuration.
- **Reproduction** — minimal steps or a proof of concept.
- **Impact** — what an attacker gains, and what access or user interaction it requires.
- **Suggested fix or mitigation**, if you have one.
- **Disclosure intent** — whether and when you plan to publish.

**Never include real secrets, access tokens, private keys, or private manuscript or creative content in a report.** Use redacted or synthetic data in reproductions.

## Scope

This policy covers the open-source code in this repository — `apps/`, `crates/`, `packages/`, `modules/`, `schemas/`, `tooling/`, and workflows under `.github/workflows/` — and the artifacts published from it, including the `nexus42` binary and the `@42ch/nexus-contracts` npm package.

Vulnerabilities in third-party dependencies are in scope when they affect how Nexus uses those dependencies: report them here as well, and we will coordinate with the upstream project.

Issues in other repositories or services are out of scope for this policy; report those through their own channels.

## Coordinated disclosure

Maintainers investigate privately through the advisory, prepare a fix, and coordinate publication with you. We will credit you in the published advisory unless you prefer to remain anonymous. Please keep the details private until a fix is released and the advisory is published; if you have a disclosure deadline, tell us in the report so we can work with you on timing.

## Contact

Security reports: GitHub Private Vulnerability Reporting (link above). For conduct concerns, see [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
