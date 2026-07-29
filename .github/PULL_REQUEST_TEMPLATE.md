## Summary
<!-- What does this PR change and why? -->

## Type of change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] Refactor / chore

## Checklist
- [ ] I ran `pnpm typecheck` and `pnpm test`
- [ ] I ran `cargo test` in `src-tauri` (or `cargo check` for docs-only)
- [ ] User-facing strings go through `src/i18n/messages.ts` (en + zh)
- [ ] No `window.confirm` / `prompt` / `alert` for product dialogs
- [ ] Docs / `docs/superpowers/` updated if behavior changed
- [ ] No secrets (`secrets.json`, tokens) included
- [ ] `pnpm check:brand` passes (or remaining violations are expected upstream-history)
