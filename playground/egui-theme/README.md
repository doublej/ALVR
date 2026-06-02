# ALVR Theme Studio (egui playground)

A standalone eframe app for designing the dashboard's egui look — **typography,
color and elements** — then exporting paste-ready `theme.rs`. Edit on the left,
preview the real widgets in the middle, copy the generated theme on the right.

## Design direction — "engineering instrument"

Dark, precise, restrained. Three bundled font families carry the voice:

| Role | Font | Used for |
|------|------|----------|
| Display | **Space Grotesk** Medium | headings, wordmark, eyebrow labels |
| Body | **IBM Plex Sans** Regular / SemiBold | labels, prose |
| Mono | **IBM Plex Mono** Regular / Medium | every number, stat tile, log line |

Plus: a cooler neutral ramp, a brighter cobalt accent, two-tier radii
(widgets vs cards), hairline borders, soft drop shadows, and a tighter
spacing rhythm.

## Run

```bash
cd playground/egui-theme
cargo run
```

## Architecture

- `typography.rs` — font installation (once at startup) + the live `TypeScale`.
- `params.rs` — `ThemeParams` (the tunable theme) + `apply()` (the source of
  truth `theme::set_theme` should grow into) + `export_rust()`.
- `editor.rs` — left panel controls.
- `gallery.rs` — center preview (cards, stat tiles, pills, segmented controls…).
- `main.rs` — app shell, panels, backdrop gradient, export panel.

## Migrate into the build

1. Tune until you like it (typography + colors + shape).
2. **📋 Copy theme.rs** (right panel).
3. In `alvr/gui_common/`:
   - replace `src/theme.rs` consts + `set_theme()` with the exported code,
   - copy `assets/fonts/*.ttf` into the crate and add a `set_fonts()` (mirror
     `typography::install_fonts`); call it once where `set_theme` is called.

## Fonts & licensing

Bundled TTFs live in `assets/fonts/` and are SIL Open Font License 1.1
(`OFL-SpaceGrotesk.txt`, `OFL-IBMPlex.txt`). egui needs raw TTF/OTF (not woff2).

## Known divergences (intentional)

- `SWITCH/SCROLLBAR_DOT_DIAMETER` are compile-time consts read by the `switch`
  component, so the live switch shows the default size; the export carries the value.
- `graph_colors` are display-only (shown as a legend), not tuned here.
- `apply()` mirrors the deprecated `ctx.set_style`/`set_visuals` calls on purpose,
  so the export is a 1:1 paste into the current `theme.rs`.
