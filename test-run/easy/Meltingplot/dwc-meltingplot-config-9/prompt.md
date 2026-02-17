# Meltingplot/dwc-meltingplot-config-9

Meltingplot/dwc-meltingplot-config (#9): Fix double ZIP in CI artifact by extracting plugin ZIP before upload

Fix the CI artifact packaging so the published plugin download is a single ZIP with the correct plugin structure. Ensure the artifact does not contain a nested ZIP and remains compatible with the downstream install script.
