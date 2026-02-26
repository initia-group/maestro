# Theming

Maestro ships with 3 built-in themes. Themes control all colors and styles across the UI.

## Selecting a Theme

Set in your config:

```toml
[ui.theme]
name = "gruvbox"
```

Available themes: `"default"` (or `"dark"`), `"light"`, `"gruvbox"`. Unknown names fall back to the dark theme.

---

## Built-in Themes

### Dark (default)

The default theme with a dark blue-gray background and cyan/green accents.

- **Background**: RGB(30, 30, 40) — very dark blue-gray
- **Text**: White
- **Accents**: Cyan project headers, green running indicators
- **Selection**: RGB(60, 60, 80) — medium gray-blue highlight

Best for dark terminal backgrounds and low-light environments.

### Light

Light background with black text, designed for bright environments.

- **Background**: RGB(240, 240, 245) — near-white
- **Text**: Black
- **Accents**: Green headers, blue mode indicators
- **Selection**: RGB(200, 210, 230) — light blue highlight

Best for light terminal themes and daytime use.

### Gruvbox

Warm earth tones inspired by the popular [Gruvbox](https://github.com/morhetz/gruvbox) color scheme.

- **Background**: RGB(40, 40, 40) — dark warm gray
- **Text**: RGB(235, 219, 178) — warm cream
- **Accents**: Aqua (RGB 104, 157, 106) project headers, orange command mode
- **Palette**: Red, green, yellow, blue, aqua, orange

Excellent contrast and readability for extended sessions.

---

## Theme Elements

### Agent Status Colors

Each agent state has a distinct color used for the status symbol in the sidebar and terminal pane title:

| State | Symbol | Color |
|-------|--------|-------|
| Spawning | `○` | Gray |
| Running | `●` | Green |
| Waiting | `?` | Yellow |
| Idle | `-` | Dark Gray |
| Completed | `✓` | Light Green |
| Errored | `!` | Red |

### Sidebar Row Backgrounds

Running, waiting, completed, and errored agents get subtle row tints in the sidebar for quick visual scanning:

| State | Tint (dark theme) |
|-------|-------------------|
| Running | Dark green RGB(25, 45, 28) |
| Waiting | Dark amber RGB(48, 38, 18) |
| Completed | Dark cyan RGB(22, 42, 42) |
| Errored | Dark red RGB(50, 22, 22) |

Idle and spawning agents have no tint.

### Pulse Animations

Agents waiting for input use smooth pulse animations to draw attention:

**WaitingForInput** — Yellow pulse:
- Symbol cycles between dim yellow RGB(120, 90, 10) and bright yellow RGB(220, 180, 30)
- Row background pulses between dark amber and slightly brighter amber

**AskUserQuestion** — Blue/purple pulse (distinct from regular waiting):
- Symbol cycles between dim blue RGB(50, 50, 140) and bright blue RGB(100, 120, 230)
- Row background pulses between dark blue-gray and slightly brighter

The animation uses an 8-phase triangle wave with linear RGB interpolation:
- Phases 0–3: dim → bright
- Phases 4–7: bright → dim

### Status Bar

| Element | Dark | Light | Gruvbox |
|---------|------|-------|---------|
| Background | RGB(40, 40, 55) | RGB(220, 220, 230) | RGB(60, 56, 54) |
| Normal mode | Cyan, bold | Blue, bold | Blue, bold |
| Insert mode | Green, bold | Green, bold | Green, bold |
| Command mode | Yellow, bold | Orange, bold | Orange, bold |

### Command Palette

| Element | Dark | Light | Gruvbox |
|---------|------|-------|---------|
| Background | RGB(35, 35, 50) | RGB(245, 245, 250) | RGB(50, 48, 47) |
| Border | Cyan | Blue | Aqua |
| Selected item | RGB(60, 60, 90) | RGB(200, 210, 240) | RGB(80, 73, 69) |

---

## Custom Themes

Custom themes are not yet supported at the config level. Themes are defined as factory methods in Rust code.

To add a new theme:

1. Add a factory method in `src/ui/theme.rs` (e.g., `pub fn my_theme() -> Self { ... }`)
2. Add a match arm in `Theme::from_name()`:
   ```rust
   "my-theme" => Self::my_theme(),
   ```
3. Use it in config: `name = "my-theme"`

Each theme defines 50+ style and color fields covering every UI region. See `src/ui/theme.rs` and [Feature 08](features/08-theme-layout-system.md) for the complete field list.
