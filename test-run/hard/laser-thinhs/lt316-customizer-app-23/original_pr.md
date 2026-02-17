# laser-thinhs/lt316-customizer-app-23 (original PR)

laser-thinhs/lt316-customizer-app (#23): Harden tracer flow with extractable core, API guardrails, and UI presets

### Motivation
- Make the image tracer reliable, safe to run in production, and easy to split out as a standalone service by extracting pure tracing logic and adding strict validation, timeouts, and a consistent API surface. 
- Improve user experience with persistent settings, presets, clear status feedback, and basic SVG hygiene so outputs are laser-friendly.

### Description
- Add a pure-core module at `lib/tracing-core/index.ts` exporting `TracerSettingsSchema`, `TracerSettings`, `traceRasterToSvg(...)`, `TracerCoreError`, and `normalizeTracerError`, with strict MIME/size/dimension checks and sane defaults (`MAX_UPLOAD_MB=10`, `MAX_DIMENSION=2000`, `MIN_SPECK_AREA=6`).
- Implement SVG normalization and speck filtering in core (`viewBox` enforcement, unit normalization to `px`, metadata stripping, path-area based speck removal) and provide a safe fallback SVG when raster tracing is not available.
- Harden API at `src/app/api/tracer/route.ts` to attach a per-request `requestId` (UUID), log request lifecycle, enforce a `TRACER_TIMEOUT_MS` guard (default 15000ms), sanitize errors, and return a consistent envelope `{ requestId, ok, result?, error? }` without exposing raw stacks.
- Add a client UI page at `src/app/tracer/page.tsx` with localStorage persistence for last settings, a `Presets` dropdown (Laser Engrave, Laser Cut, Photo Logo), an `outlineMode` toggle, clear status banners (Uploading/Tracing/Done/Failed), and `Download .svg` / `Copy SVG` actions.
- Add tests: unit tests for `TracerSettingsSchema` defaults and `normalizeSvg` behavior at `src/__tests__/tracing-core.test.ts`, and an integration test that posts a tiny PNG to the tracer API at `src/__tests__/tracer.route.integration.test.ts`.

### Testing
- Ran unit and integration tests with `npm test -- --runInBand src/__tests__/tracing-core.test.ts src/__tests__/tracer.route.integration.test.ts`, and both test suites passed. 
- Ran `npm run lint` which failed in this environment because `eslint` is not installed, causing `next lint` to error. 
- Attempted a headless browser check to capture the `/tracer` page which failed in the environment due to the Playwright/Chromium process crashing, but this did not affect the Jest test results.

------
[Codex Task](https://chatgpt.com/codex/tasks/task_e_6993a8017cac8326b447ccd4b3ebeaa1)
