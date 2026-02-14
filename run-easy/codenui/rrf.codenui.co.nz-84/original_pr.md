# codenui/rrf.codenui.co.nz-84 (original PR)

codenui/rrf.codenui.co.nz (#84): Refine site detail panel headings and site-type badge

### Motivation
- Reduce redundant headings and visual clutter in the site details panel by removing the extra `Details` heading and the licence/carrier count badges, and surface a concise site-type label (`Solo site` / `Shared site`).

### Description
- Updated the HTML template in `rrf.py` to compute `siteTypeLabel = carrierCount === 1 ? "Solo site" : "Shared site"` and replaced the licence/count badges with a single badge showing `siteTypeLabel`, and removed the separate `Details` heading from the detail card.

### Testing
- Ran `python -m py_compile rrf.py` which passed, regenerated the HTML with `python rrf.py --html-only` (wrote `index.html`), and served the site with `python -m http.server 4173` and captured a Playwright screenshot for visual verification (all succeeded).

------
[Codex Task](https://chatgpt.com/codex/tasks/task_e_69900a4e2e408327ae43215f600d2dbc)
