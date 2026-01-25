# Repository Guidelines

## Project Structure & Module Organization
- `src/` contains the Rust CLI and core logic. Entry points: `src/main.rs` (CLI) and `src/lib.rs` (core flow). Feature modules live under `src/` (config, build, checksums, changelog formatting, release providers).
- `src/release_provider/` and `src/checksummer/` are organized by backend and algorithm.
- `docs/` hosts the Astro/Starlight documentation site, with content in `docs/src/content/`.
- `rlsr.yml` is the primary runtime configuration; `rlsr.sample.yml` shows expected structure.
- `dist/` and `target/` are generated outputs for release artifacts and Cargo builds.

## Build, Test, and Development Commands
- Prefer `just` targets from `Justfile` whenever available (e.g., `just build`, `just release`, `just build-linux`, `just docs-serve`) before invoking `cargo` directly.
- `cargo build`: build the Rust binary (use if no `just` target fits).
- `cargo run -- --config rlsr.yml`: run locally with a specific config.
- `cargo test`: run unit tests (primarily under checksum modules).
- `cargo clippy`: lint Rust code (required after Rust edits).
- `just docs-serve` or `cd docs && npm run dev`: run the docs site locally.

## Coding Style & Naming Conventions
- Rust code follows standard formatting via `rustfmt` (`cargo fmt`). Keep 4-space indentation and idiomatic snake_case for functions/modules.
- YAML config keys should mirror `rlsr.sample.yml` and stay consistent with existing naming.
- Templates live in `src/changelog_formatter/tmpls/`; keep template files lowercase with underscores.

## Testing Guidelines
- Use `cargo test` for unit tests. Tests currently live in-module with `#[cfg(test)]` and functions named `test_*`.
- Add tests alongside the module you change, especially for checksum or release logic.
- After modifying Rust code, run `cargo clippy` and fix any warnings.

## Commit & Pull Request Guidelines
- IMPORTANT: Check if the repo uses `jj` and use `jj` for commits/operations; otherwise use `git`.
- Commit messages follow Conventional Commits (examples: `feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`, optional scopes like `feat(checksum): ...`). Also a commit body(with bullet points) wherever appropriate.
- PRs should include a short description, testing notes, and doc updates if behavior changes. Include screenshots for docs UI changes.

## Configuration & Release Notes
- Use `rlsr.yml` for real runs and `rlsr.sample.yml` as the reference. Release artifacts are written to `dist/` and checksums are generated during publish flows.
