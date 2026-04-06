# Contributing

## Branch and pull request workflow

1. Use a **dedicated branch per feature or fix** (for example `feature/render-pass`, `fix/clippy-warnings`). Avoid committing large, unrelated changes directly to the default branch.
2. If you use a **fork**, add the canonical repository as remote `upstream` and keep your default branch in sync with `upstream` before branching.
3. Open **pull requests against the upstream default branch** (for example `main`) with a clear description and links to related issues when applicable.
4. **Rebase or merge from upstream** as needed so the PR applies cleanly.
5. Ensure **CI passes** (`cargo fmt`, `cargo clippy`, `cargo test`) before requesting review.

## Code style

- Run `cargo fmt --all` and `cargo clippy --workspace -- -D warnings` locally when possible.
- Prefer small, reviewable PRs over large mixed-topic changes.
