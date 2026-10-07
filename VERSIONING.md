# Versioning

Minimap uses [semantic versioning](https://semver.org/) for releases and git tags to mark them.

## Quick reference

- **Current version:** `0.1.0` (in `Cargo.toml`)
- **Bump version:** Edit `Cargo.toml` at the root (all crates inherit via `version.workspace = true`)
- **Tag a release:** `git tag -a v0.2.0 -m "Release 0.2.0: <description>"` after merging to `master`
- **See releases:** `git tag --list` or `git log --oneline --decorate | grep tag`

## Semantic versioning

- **MAJOR** (1.0.0, 2.0.0, …): Breaking changes to the data model, schema, sync protocol, or app data format
- **MINOR** (0.1.0, 0.2.0, …): New features (spec implementations), non-breaking changes
- **PATCH** (0.1.1, 0.1.2, …): Bug fixes, performance improvements, refactors

While pre-1.0, breaking changes to schema or data happen in minor versions (e.g. M1 → M2 is 0.2.0).

## Workflow

### 1. During development
Track work in `docs/progress.md` by milestone. Each milestone is a candidate for a minor version bump.

### 2. Before a release

**Update version in one place:**
```bash
# Edit Cargo.toml, change:
# version = "0.1.0"
# to:
# version = "0.2.0"

cargo fmt
git add Cargo.toml
git commit -m "Bump to 0.2.0"
```

All member crates (`src-tauri`, `crates/minimap-*`, `ui`) automatically inherit this version via the workspace package definition.

**Verify:**
```bash
cargo metadata --format-version=1 | jq '.packages[0].version'
```

### 3. Tag the release

After the version-bump commit is merged to `master`:

```bash
git tag -a v0.2.0 -m "Release 0.2.0: Spec 24 (remove demo data), objective colours, task board"
git push origin v0.2.0
```

Show tagged commits:
```bash
git log --oneline --decorate | head -20
```

### 4. Optional: Update CHANGELOG.md

Create one if you want a user-facing release summary (include link in README.md).

```markdown
# Changelog

## [0.2.0] - 2026-10-07
### Added
- Spec 24: Remove demo data (Settings → Data & backup)
- Objective colours: each objective gets a colour for visual grouping (ADR-0012)
- Tasks board: drag-and-drop Kanban board with list fallback

### Fixed
- Contrast of objective colours on all 17 themes (raised DARK_SL, LIGHT_SL)

## [0.1.0] - 2026-09-15
### Added
- Initial release (M0–M4: scaffolding, core data, graph engine, people & exec layer, review & security)
```

## Implementation notes

- **Workspace setup** (`Cargo.toml`): All member crates use `version.workspace = true`, so a single edit propagates everywhere
- **Built-in version** (`src-tauri/src/commands/export.rs`): The app exports and restore backups record `app_version: env!("CARGO_PKG_VERSION")`, so version history is preserved in data
- **No `publish`**: The workspace has `publish = false` (local app, not on crates.io)

## Automation (optional)

If you want to automate version bumps:

```bash
# Install cargo-edit
cargo install cargo-edit

# Bump all crates (via workspace)
cargo set-version 0.2.0 --all

# Or edit the root Cargo.toml and let workspace inherit
```

For releases, you can use GitHub Actions or a local script to:
1. Bump version
2. Tag and push
3. Build the installer (Tauri handles this)

For now, manual git tag is fine.

## Examples

**Going from 0.1.0 → 0.2.0 (new spec or milestone)**
```bash
# Edit Cargo.toml: version = "0.2.0"
git add Cargo.toml
git commit -m "Bump to 0.2.0"
git tag -a v0.2.0 -m "Release 0.2.0: Specs 24–26 complete"
git push origin master v0.2.0
```

**Going from 0.2.0 → 0.2.1 (bug fix)**
```bash
# Fix the bug, commit it, then:
# Edit Cargo.toml: version = "0.2.1"
git add Cargo.toml
git commit -m "Bump to 0.2.1: fix sync edge case"
git tag -a v0.2.1 -m "Release 0.2.1: sync fix"
git push origin master v0.2.1
```

## Linking from docs

Reference this file from:
- **README.md** (near "Building" or "Contributing"): "See [VERSIONING.md](VERSIONING.md) for how releases are versioned and tagged."
- **docs/progress.md** (at the top): "Version numbers track milestones; see [VERSIONING.md](../VERSIONING.md)."

---

**Last updated:** 2026-10-07 (schema stable; demo data removal added; M4 complete)
