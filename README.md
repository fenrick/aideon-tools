# aideon-tools

Command line tools for Aideon Praxis.

## Development

This repository targets the Rust 2024 edition and uses CI workflows to enforce formatting, linting, and testing across Linux, macOS, and Windows. Before opening a pull request:

- Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features --locked`.
- Ensure commit messages follow the [Conventional Commits](https://www.conventionalcommits.org/) specification. Pull requests trigger automated commit linting via Commitlint, and releases rely on semantic-release to compute version bumps from these messages.

Release artifacts are generated for Linux, macOS (Apple Silicon), and Windows when changes land on `main`.
