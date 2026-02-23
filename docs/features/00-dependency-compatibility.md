# Dependency Compatibility Matrix

## Critical Finding

**The versions specified in the original PLAN.md (`ratatui 0.29` + `tui-term 0.3` + `vt100 0.15` + `crossterm 0.28`) are NOT compatible.**

`tui-term 0.3.x` depends on `ratatui-core ^0.1.0` and `ratatui-widgets ^0.3.0`, which are sub-crates introduced in `ratatui 0.30.0`. They don't exist in the `ratatui 0.29` ecosystem. Additionally, `tui-term 0.3.x` requires `vt100 ^0.16.2`, not `0.15`.

## Recommended Version Set (Option B — Latest)

**This is what Maestro should use.**

```toml
[dependencies]
ratatui = { version = "0.30", features = ["crossterm"] }
crossterm = "0.29"
tui-term = { version = "0.3", features = ["vt100"] }
vt100 = "0.16"
portable-pty = "0.9"
```

| Crate | Version | Latest Stable | Notes |
|---|---|---|---|
| `ratatui` | **0.30.0** | 0.30.0 | New modular architecture (ratatui-core + ratatui-widgets) |
| `crossterm` | **0.29.0** | 0.29.0 | Required by ratatui 0.30 |
| `tui-term` | **0.3.1** | 0.3.1 (2026-01-25) | Actively maintained, 12 contributors |
| `vt100` | **0.16.2** | 0.16.2 | Required by tui-term 0.3.x |
| `portable-pty` | **0.9.0** | 0.9.0 | Battle-tested (WezTerm) |

## Alternative Version Set (Option A — If staying on ratatui 0.29)

Only use this if there's a specific reason to avoid ratatui 0.30.

```toml
[dependencies]
ratatui = { version = "0.29", features = ["crossterm"] }
crossterm = "0.28"
tui-term = { version = "0.2", features = ["vt100"] }
vt100 = "0.15"
portable-pty = "0.8"  # NOTE: must be 0.8, NOT 0.9
```

## tui-term Maintenance Status

**Actively maintained** as of February 2026:
- Last release: v0.3.1 on 2026-01-25
- 12 contributors on GitHub
- Maintainer: Kenji Berthold (@a-kenji)
- 3 open issues (low)
- No established alternatives exist; tui-term is the de facto standard for embedding terminals in Ratatui.

## API Differences Between Options

### ratatui 0.30 changes
- `ratatui::prelude::*` still works for most imports.
- The crossterm backend is now `ratatui-crossterm` (but the feature flag `crossterm` on ratatui handles this).
- Widgets API is mostly unchanged.

### tui-term 0.3 changes vs 0.2
- Depends on `ratatui-core` and `ratatui-widgets` instead of `ratatui` directly.
- Same `PseudoTerminal` widget API.
- Uses `vt100 0.16` which has minor API differences from 0.15.

### vt100 0.16 changes vs 0.15
- `screen.contents()` API remains the same.
- `Parser::new(rows, cols, scrollback)` unchanged.
- Some internal improvements to ANSI parsing accuracy.

## Impact on Feature Plans

All feature plans in this directory should use the **Option B versions**. Specifically:
- **Feature 01** (Scaffold): `Cargo.toml` should use Option B versions.
- **Feature 04** (PTY): `portable-pty = "0.9"` (already correct).
- **Feature 10** (Terminal Pane): `tui-term 0.3` API, `vt100 0.16`.
- **Feature 15** (Scrollback): `vt100 0.16` scrollback API.

## Verification

After setting up `Cargo.toml`, run:
```bash
cargo check
```
If dependency resolution fails, check `cargo tree` to identify conflicting transitive dependencies.
