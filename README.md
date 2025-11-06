# aideon-tools

Command line tools for Aideon Praxis.

## Usage

```bash
aideon-tools sync \
  --from jsonld input.jsonld \
  --to xlsx output.xlsx \
  --context context.json
```

Refer to `aideon-tools sync --help` for the complete list of supported
conversions and options.

## Logging

The CLI emits structured logs via [`tracing`](https://docs.rs/tracing) using the
`--log-level` flag (defaults to `info`). The level can also be overridden with
`RUST_LOG`, which takes precedence when set:

```bash
RUST_LOG=aideon_tools=debug aideon-tools sync --from rdf --to jsonld --log-level warn
```

## Development

This repository targets the Rust 2024 edition and uses CI workflows to enforce formatting, linting, and testing across Linux, macOS, and Windows. Before opening a pull request:

- Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features --locked`.
- Ensure commit messages follow the [Conventional Commits](https://www.conventionalcommits.org/) specification. Pull requests trigger automated commit linting via Commitlint, and releases rely on semantic-release to compute version bumps from these messages.

Release artifacts are generated for Linux, macOS (Apple Silicon), and Windows when changes land on `main`.
