# ALVR egui Theme Playground

A standalone eframe app for experimenting with the dashboard's egui theme
without rebuilding the whole streamer. Edit on the left, preview in the middle,
copy paste-ready code on the right.

## Why it's separate

It lives **outside** the ALVR workspace (`members = ["alvr/*"]`) and declares its
own empty `[workspace]` table, so it never enters `cargo build` / `clippy` / CI.
It depends on the real `alvr_gui_common` by path, so:

- it starts from the **exact** live theme (`alvr_gui_common::theme`),
- it renders the **real** components (`switch`, `button_group_clicked`),
- the only divergence is that `apply()` is a value-driven copy of
  `theme::set_theme` so the constants are tunable at runtime.

## Run

```bash
cd playground/egui-theme
cargo run
```

## Migrate changes back into the build

1. Tune values until you like the preview.
2. Hit **📋 Copy code** (right panel).
3. Paste over the matching parts of `alvr/gui_common/src/theme.rs`:
   - the `pub const` color/scalar block,
   - the `log_colors` module,
   - splice the commented spacing lines into `set_theme()`.

## Known divergences (intentional, to keep the playground simple)

- `SWITCH_DOT_DIAMETER` / `SCROLLBAR_DOT_DIAMETER` are compile-time consts read
  directly by the `switch` component, so the live switch always shows the
  default size; the slider only affects the exported code.
- `graph_colors` are display-only (not tuned) — shown as a legend for reference.
