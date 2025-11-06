# Agent Instructions for aideon-tools

## Code Quality
- Follow Rust 2024 idioms and keep modules under the `aideon::tools` namespace when adding new code.
- Prefer small, single-responsibility functions and modules; refactor shared logic into reusable helpers.
- Use clear, descriptive identifiers and provide concise Rustdoc comments for all public APIs, including inputs, outputs, and side effects.
- Implement robust error handling with contextual messages and avoid silently ignoring failures.

## Formatting & Linting
- Run `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` before committing any Rust changes.
- Keep GitHub workflows, configuration files, and documentation formatted consistently (YAML: two-space indentation).

## Testing & CI
- Add or update unit and integration tests for new behavior; tests must run via `cargo test --all-features --locked` in CI.
- Ensure new workflows or scripts integrate with existing CI/release processes without regressing cross-platform builds.

## Processes & Security
- Use meaningful commit messages and ensure the working tree is clean before invoking release tooling.
- Never store secrets in the repository; reference GitHub Actions secrets or environment variables instead.
- Prefer HTTPS endpoints for external resources and document any required environment variables.

## Documentation & Releases
- Update README or docs when changing the public surface area or workflows.
- When modifying release automation, verify semantic-release configuration remains consistent across Linux, macOS, and Windows artifacts.
