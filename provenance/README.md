# Upstream provenance

`upstreams.json` freezes the imported Grok App commit and the OMP gitlink baseline. `omp-patches.json` is the ledger for local OMP changes.

## Remote roles and publication state

- The desktop superproject's intended `origin` is the writable team repository. It is not configured yet because that repository is still planned, not published.
- Superproject `grok-app-upstream` points to the read-only Grok App upstream.
- Submodule `origin` is configured to the planned writable team Fork URL. The URL is recorded locally, but remote publication is not yet complete.
- Submodule `upstream` points to the read-only official OMP repository.

The checker validates local remote configuration, the committed gitlink, the checked-out submodule commit, and both frozen JSON records without contacting GitHub. A successful local check can therefore still report publication concerns. Remote publication remains a separate release prerequisite.

Every OMP commit that is not part of official upstream history requires an entry in `omp-patches.json` describing the local patch.
