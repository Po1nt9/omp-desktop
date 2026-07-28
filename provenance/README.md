# Upstream provenance

`upstreams.json` freezes the imported Grok App commit and the OMP gitlink baseline. `omp-patches.json` is the ledger for local OMP changes.

## Remote roles and publication state

- The desktop superproject's `origin` is the published writable team repository recorded by `desktop.repository`.
- Superproject `grok-app-upstream` points to the read-only Grok App upstream.
- Submodule `origin` is the published writable team Fork recorded by `omp.forkRemote`.
- Submodule `upstream` points to the read-only official OMP repository.

The checker validates publication-state-dependent local remote configuration, the committed gitlink, the checked-out submodule commit, and both frozen JSON records without contacting GitHub. When desktop publication is `published`, superproject `origin` must exactly match `desktop.repository`; while it is `planned`, any unexpected `origin` is rejected. OMP publication continues to be validated locally through the `.gitmodules` URL and nested repository remotes.

Both recorded repositories are now published, and the pinned OMP commit is available from the Fork. The Task 14 remote-publication blocker is resolved.

Every OMP commit that is not part of official upstream history requires an entry in `omp-patches.json` describing the local patch.
