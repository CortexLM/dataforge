# markuplint/markuplint-3177

markuplint/markuplint (#3177): refactor(ml-config)!: simplify merge algorithm for v5

## Summary

- Replace `deepmerge` with shallow merge (`{...a, ...b}`) for all object merging
- Change rule array values from concatenation to override (right-side wins), consistent with ESLint/Biome
- Change pretenders `data` merge from override to append, `files`/`imports` remain override
- Remove deprecated `option` (singular) field support and `RuleV2`/`RuleConfigV2`/`AnyRuleV2` types
- Remove array support for `specs` config field
- Simplify `concatArray` helper
- Add comprehensive test coverage (62 tests, up from 28)
- Update all documentation (ARCHITECTURE, maintenance guide, SKILL, merge-config spec)
- Add v4→v5 migration guide sections for merge behavior changes
- Fix `file-resolver` test fixtures using deprecated `option` field

## BREAKING CHANGES

- Rule array values now override instead of concatenate when merging configs
- Rule options use shallow merge instead of deep merge
- Pretender `data` arrays now append instead of override
- Deprecated `option` (singular) field on `RuleConfig` is no longer supported — use `options` (plural)
- Deprecated `RuleV2`, `RuleConfigV2`, `AnyRuleV2` types removed
- `specs` config field no longer accepts array form

## Test plan

- [x] `yarn lint` — 1383 files, 0 issues
- [x] `yarn build` — 37 projects all pass
- [x] `yarn test` — 1511 passed (1 flaky CLI timeout, unrelated)
- [x] `npx vitest run packages/@markuplint/ml-config/` — 62 tests pass
- [x] `npx vitest run packages/@markuplint/file-resolver/` — 24 tests pass

🤖 Generated with [Claude Code](https://claude.com/claude-code)
