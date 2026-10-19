# Contributing to Perigee

Thank you for your interest in contributing to **Perigee**! We are excited to have you as part of our community.

As a project in the **Stellar Wave Program**, we value collaboration and clear communication. Please use the following guides to help you get started:

## 📖 Guides

- [**Development & Setup**](./docs/development.md): How to set up the monorepo and our coding standards.
- [**How to Open a Pull Request**](./docs/pull-requests.md): Our step-by-step workflow for submitting code.
- [**Reporting Issues**](./docs/issues.md): How to report bugs or suggest new features.

## 🧭 Runtime conventions

These are project-wide rules enforced by code review. New code must follow them.

### Timestamps are always UTC

All timestamps emitted by the API and embedded in report payloads are **UTC** and rendered as RFC 3339 strings with the `+00:00` offset (or a trailing `Z`).

- **Use `chrono::Utc::now()`** for any new timestamp.
- **Never** use `std::time::SystemTime::now()` or `chrono::Local::now()` in report or API payloads — they do not carry timezone information and produce off-by-offset reports.
- New fields that carry a timestamp must be typed `chrono::DateTime<chrono::Utc>` so the serialization layer emits a timezone-aware value.

If you need to migrate an existing `u64` (Unix epoch seconds) field, prefer upgrading the field type to `DateTime<Utc>` so the wire format is self-documenting.

### Structured logging via `tracing`

The `core` server uses [`tracing`](https://docs.rs/tracing) + `tracing-subscriber`. Application code should **never** write to `stdout`/`stderr` directly for diagnostic output:

- Use `tracing::{info, warn, error, debug, trace}!` macros with structured key/value fields (e.g. `tracing::info!(contract_id = %id, "received request")`).
- Severity is controlled by the `RUST_LOG` env var (e.g. `RUST_LOG=info,core=debug`).
- Set `LOG_FORMAT=json` in production / container deployments to emit line-delimited JSON for log aggregators; omit it for the default pretty text format during local development.
- The CLI subcommands (`merkle`, `compare`, `export`, `restore`) intentionally keep `println!`/`eprintln!` because their output is meant for shell pipelines, not log aggregation.

### Graceful shutdown

The `core` HTTP server listens for `SIGINT` (Ctrl-C) and `SIGTERM` on Unix targets. On either signal it stops accepting new connections and drains in-flight requests before exiting. Any long-lived background task spawned from `main` should be cancelable via `tokio::select!` or `CancellationToken` so that the process exits cleanly within a few seconds of the signal.

## 📝 Commit message convention

This repo enforces [Conventional Commits](https://www.conventionalcommits.org/).
The `commit-msg` git hook (installed via Husky) validates every commit message
automatically once you run `yarn install` at the repo root.

### Format

```
<type>(<optional scope>): <short description>

[optional body]

[optional footer(s)]
```

### Allowed types

| Type | When to use |
|---|---|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `style` | Formatting, whitespace — no logic change |
| `refactor` | Code change that is neither fix nor feat |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `build` | Build system / dependency changes |
| `ci` | CI configuration |
| `chore` | Other maintenance |
| `revert` | Reverts a previous commit |
| `contract` | Soroban contract changes |
| `agent` | Agent fleet / runtime changes |

### Breaking changes

Append `!` after the type/scope, or add a `BREAKING CHANGE:` footer:

```
feat(vault)!: require explicit policy version in deposit call

BREAKING CHANGE: the `deposit` endpoint now requires a `policy_version`
field. Clients on v0 must migrate before upgrading.
```

### Examples

```
feat(agent): add on-chain reputation score to agent identity
fix(web): resolve wallet modal z-index on mobile
docs(contracts): document policy_vault allowlist format
ci: add commitlint workflow for PR validation
contract(policy_vault): enforce drawdown threshold on rotation
```

## 📋 Changelog

`CHANGELOG.md` is generated automatically from commit history.

```bash
# Append entries since the last git tag
yarn changelog

# Rebuild from scratch (first release)
yarn changelog:first
```

Entries are written for: `feat`, `fix`, `perf`, `revert`, `docs`, `refactor`,
`contract`, and `agent`. Build, CI, chore, style, and test commits are hidden.

## 🤝 Questions?

Feel free to open an **Issue** or reach out to the **SoroLabs** team. Let's build the best Soroban developer tools together!
