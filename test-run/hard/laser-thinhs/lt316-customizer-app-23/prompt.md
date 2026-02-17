# laser-thinhs/lt316-customizer-app-23

laser-thinhs/lt316-customizer-app (#23): Harden tracer flow with extractable core, API guardrails, and UI presets

Harden the image tracer feature so it is reliable in production and can be separated as a standalone service. Enforce strict validation of uploads (MIME type, size, and dimensions) with sane defaults and reject invalid inputs. Ensure tracing produces laser-friendly SVGs by normalizing output (consistent viewBox and units, stripping metadata) and removing tiny speck artifacts, and provide a safe fallback SVG when tracing isn’t available.

Make the tracer API return a consistent response envelope that includes a per-request ID, success status, result or error, and does not expose raw stack traces. Add request timeouts and lifecycle logging, and sanitize errors returned to clients.

Improve the tracer UI with persistent settings, user-selectable presets, an outline mode toggle, clear status feedback (uploading, tracing, done, failed), and actions to download or copy the generated SVG.

Add automated tests to cover default settings validation, SVG normalization behavior, and an integration test that posts a small PNG to the tracer API.
