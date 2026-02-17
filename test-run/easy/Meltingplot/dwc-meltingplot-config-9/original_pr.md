# Meltingplot/dwc-meltingplot-config-9 (original PR)

Meltingplot/dwc-meltingplot-config (#9): Fix double ZIP in CI artifact by extracting plugin ZIP before upload

upload-artifact@v4 wraps uploaded files in a ZIP, so uploading the
plugin .zip produced a ZIP-inside-ZIP that breaks DWC's install script.
Extract the plugin ZIP contents first so the downloaded artifact is a
single ZIP with the correct plugin structure.

https://claude.ai/code/session_01UovtPLX8SozwpF3mm98NKP
