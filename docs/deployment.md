# Deployment and Releases

Maestro uses GitHub Actions for CI/CD and distributes pre-built binaries via GitHub Releases and Homebrew.

## Overview

```
Cargo.toml (version) ──→ GitHub Actions (workflow_dispatch)
                              │
                    ┌─────────┼──────────┐
                    ▼         ▼          ▼
               Tag + Build  Release   Update Homebrew Tap
               (4 platforms) (draft)  (auto-update formula)
```

## Supported Platforms

| Target | Runner | Cross-compiled |
|--------|--------|----------------|
| `x86_64-apple-darwin` | macos-14 | No |
| `aarch64-apple-darwin` | macos-14 | No |
| `x86_64-unknown-linux-gnu` | ubuntu-latest | No |
| `aarch64-unknown-linux-gnu` | ubuntu-latest | Yes (via `cross`) |

---

## CI Pipeline

**File:** `.github/workflows/ci.yml`

Runs on every push to `main` and on pull requests targeting `main`.

| Step | Command |
|------|---------|
| Format check | `cargo fmt --check` |
| Lint | `cargo clippy -- -D warnings` |
| Test | `cargo test` |

All three must pass for the PR to be mergeable.

## Release Workflow

**File:** `.github/workflows/release.yml`

Triggered manually via **workflow_dispatch** (the "Run workflow" button in GitHub Actions).

### Jobs

#### 1. `prepare`

- Reads the version from the `version` input, or from `Cargo.toml` if left blank
- Validates semver format (`N.N.N`)
- Creates and pushes the git tag `vN.N.N` (skips if tag already exists)
- Outputs `version` and `tag` for downstream jobs

#### 2. `build` (4 parallel jobs)

- Checks out code and installs the Rust toolchain
- For `aarch64-unknown-linux-gnu`: installs `cross` for cross-compilation
- Builds a release binary: `cargo build --release --target <target>`
- Packages the binary with README, LICENSE, and default.toml into a `.tar.gz`
- Generates a SHA256 checksum file
- Uploads both as build artifacts

#### 3. `release`

- Downloads all build artifacts
- Creates a GitHub Release with auto-generated release notes
- Attaches all `.tar.gz` archives and `.sha256` checksum files

#### 4. `update-homebrew`

- Downloads SHA256 checksum files from build artifacts
- Generates a new `Formula/maestro.rb` with the correct version and checksums
- Clones `initia-group/homebrew-tap`, commits the formula update, and pushes
- Requires the `TAP_GITHUB_TOKEN` secret

---

## Homebrew Distribution

### Tap Repository

- **Repo:** [initia-group/homebrew-tap](https://github.com/initia-group/homebrew-tap)
- **Formula:** `Formula/maestro.rb`
- **Install:** `brew tap initia-group/tap && brew install maestro`

The formula is automatically updated by the release workflow. It uses platform detection (`on_macos`/`on_linux` + `on_arm`/`on_intel`) to download the correct binary.

### Local Template

A template copy of the formula lives at `homebrew/maestro.rb` in the main repo. This is a reference only — the live formula is auto-generated in the tap repo.

---

## Required Secrets

| Secret | Where | Purpose |
|--------|-------|---------|
| `TAP_GITHUB_TOKEN` | `initia-group/maestro` repo settings | Personal access token with write access to `initia-group/homebrew-tap`. Used by the release workflow to push formula updates. |

To create the token:
1. GitHub Settings > Developer settings > Personal access tokens > Fine-grained tokens
2. Grant **Contents: Read and write** access to `initia-group/homebrew-tap`
3. Add it as a repository secret named `TAP_GITHUB_TOKEN` in `initia-group/maestro`

---

## Making a Release

### Step by Step

1. **Update the version** in `Cargo.toml`:
   ```toml
   version = "0.2.0"
   ```

2. **Commit and push** to `main`:
   ```sh
   git add Cargo.toml
   git commit -m "Bump version to 0.2.0"
   git push origin main
   ```

3. **Trigger the release:**
   - Go to [Actions > Release > Run workflow](https://github.com/initia-group/maestro/actions/workflows/release.yml)
   - Leave version blank (reads from Cargo.toml) or enter `0.2.0`
   - Click **Run workflow**

4. **Wait** for all jobs to complete (build, release, update-homebrew).

5. **Verify** the release:
   ```sh
   # Check GitHub Release
   gh release view v0.2.0

   # Check Homebrew
   brew update
   brew upgrade maestro
   maestro --version
   ```

### Quick Reference

```sh
# Bump version
sed -i '' 's/version = "0.1.0"/version = "0.2.0"/' Cargo.toml
git add Cargo.toml && git commit -m "Bump version to 0.2.0" && git push

# Trigger release via CLI (alternative to GitHub UI)
gh workflow run release.yml -f version=0.2.0
```
