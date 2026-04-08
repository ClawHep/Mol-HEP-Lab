# Review Notes — count-events-in-a-simple-20260326

## Phase 5: Documentation Review

**Date:** 2026-03-26
**Artifact:** ANALYSIS_NOTE.md
**Verdict:** PASS (after iteration)
**Iterations:** 2

### 5-Bot Review Summary

| Reviewer | Initial verdict | Issues found | Resolution |
|----------|----------------|--------------|------------|
| Physics | ITERATE | A1: synthetic data scope; A2: no plots; A3: unphysical kinematics undiscussed | A1/A2 downgraded by arbiter (note already scoped as methodology demo); A3 addressed with kinematic note |
| Critical | ITERATE | A1: SE_sub ratio wrong (4.72→3.16); A2: column count wrong (7→8); B2/B3: no references.bib | All fixed in iteration |
| Constructive | PASS | B1: tag-quality systematic asymmetry; B3: column independence | Both addressed with explicit rationale |
| Rendering | FAIL | A-1: missing references.bib; B-2: double numbering risk; B-3/B-4: bibtex code block | All fixed in iteration |
| Arbiter | ITERATE | Confirmed 3 Category A: SE ratio, column count, references.bib | All 3 pre-fixed before verdict |

### Fixes Applied in Iteration 2

1. **SE_sub ratio corrected**: `~4.72 × SE_full` → `~3.16 × SE_full` (= sqrt(192/19))
2. **Column count corrected**: "Seven columns are numeric" → "Eight columns are numeric (including `weight`)"
3. **references.bib created**: BibTeX entries extracted from embedded code block to `references.bib`
4. **YAML frontmatter updated**: Added `bibliography: references.bib`, `link-citations: true`, `number-sections: false`, fixed `author` field
5. **Bibtex code block removed** from note body (would have rendered as visible code)
6. **Adjacent display math blocks**: Added blank lines between consecutive `$$...$$` blocks
7. **Kinematic asymmetries note added**: Documented that all-positive pz, forward-biased eta, and large mean py are synthetic generator artefacts
8. **Tag-quality systematic rationale added**: Explicit explanation of why the tag-quality cut variation is not included in the systematic budget (it defines the signal region)
9. **Weighted vs unweighted note added**: Clarified that primary result is unweighted count; weighted yield ≈193.9 differs by 1.9 events (0.14σ)

### Open Issues (non-blocking)

- Real data not staged at `data_dir=/private/tmp` — all results are from synthetic demonstration data
- Duplicate `event_id`=31 provenance unresolved
- No distribution plots (no figure files available in analysis)
- Wide 13-column table in Appendix A may overflow margins at build time

---

## Phase 4c: Full Unblinding Review

**Date:** 2026-03-26
**Artifact:** INFERENCE_FULL.md
**Verdict:** PASS

All validation checks passed:
- N_valid = 192 ± 1 (syst) events confirmed
- Shell wc-l cross-check: PASS
- 10% subsample diagnostic: PASS (max pull 1.63σ)
- Systematic completeness: PASS
- Operating point stability: PASS

---

## Prior Phase Reviews

| Phase | Date | Artifact | Verdict | Notes |
|-------|------|----------|---------|-------|
| 1 | 2026-03-26 | STRATEGY.md | PASS | Strategy complete; synthetic data gap documented |
| 2 | 2026-03-26 | EXPLORATION.md | PASS | Conducted on synthetic data; real data required |
| 3 | 2026-03-26 | SELECTION.md | PASS | Two iterations; N_valid=192 confirmed |
| 4a | 2026-03-26 | INFERENCE_EXPECTED.md | PASS | Closure test: all checks < 0.15σ |
| 4b | 2026-03-26 | INFERENCE_VALIDATION.md | PASS | 10% subsample diagnostic all pass |
| 4c | 2026-03-26 | INFERENCE_FULL.md | PASS | Final results confirmed |
| 5 | 2026-03-26 | ANALYSIS_NOTE.md | PASS | 3 Category A issues fixed in iteration |
