# fluxcd/helm-controller-1411

The Helm controller does not properly reconcile status conditions when a HelmRelease is already in-sync with the source. Ensure the controller evaluates and updates conditions for every reconciliation, regardless of whether the release is already at the desired state. The status should reflect the current truth of the release conditions after each reconciliation loop.
