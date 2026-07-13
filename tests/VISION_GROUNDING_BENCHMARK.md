# Vision grounding benchmark

Run the ignored `live_coordinate_benchmark` test with `CC_VISION_BENCH_MANIFEST`
pointing to a local JSON manifest and `CC_VISION_BENCH_OUTPUT` to a JSONL result.
The test uses the production image encoder and provider request path.

Each case has `image`, `target`, `category`, optional `tags`, `visible`, and
`box_px: [x, y, width, height]`. Omit `box_px` only for a deliberately absent
target (`visible: false`). Results include strict-box accuracy, pixel error,
latency, parsing, category, and tags.

A ranking run is not representative until it includes all of these independent
dimensions:

- text buttons, icon-only buttons, tiny status icons, dense rows, menus;
- light and dark surfaces, low contrast, disabled and selected states;
- 100%, 125%, 150%, and 200% display scaling;
- overlapping windows, edge targets, repeated lookalikes, and absent targets;
- browser DOM, canvas/WebGL, and native UIA-blind surfaces;
- full-screen, window crop, multiple monitors, and non-primary monitor origins.

Keep personal screenshots and API outputs outside the repository. Commit only
the harness and anonymized fixtures.
