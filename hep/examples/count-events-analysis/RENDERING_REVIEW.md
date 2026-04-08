# Rendering Review

## Summary
- **Document**: `/Users/bamboo/Githubs/ClawHep/MoltHep/analyses/count-events-in-a-simple-20260326/ANALYSIS_NOTE.md`
- **Date**: 2026-03-26
- **Compilation status**: Not attempted (pixi toolchain not available in this environment; source-level review performed)
- **Page count**: Not measured (source review only); estimated ~25–35 pages given volume of tables and text
- **Category A issues**: 1
- **Category B issues**: 4
- **Category C issues**: 3

## Compilation
- **Command**: pixi run build-pdf
- **Exit code**: Not run
- **Warnings**: See source-level analysis below
- **Errors**: See Category A issue — missing references.bib

---

## Figure Rendering
No figures are included in the document (no `![...]` image references or `\includegraphics` commands). Not applicable.

---

## Math Compilation

All LaTeX math was reviewed at source level. Overall quality is high.

**Inline math (`$...$`):** Consistently used throughout. Greek letters (`\eta`, `\varphi`, `\sigma`, `\chi`, `\mu`), subscripts, superscripts, operators (`\pm`, `\ll`, `\leq`, `\approx`, `\checkmark`), and text-mode macros (`\text{...}`) are all syntactically correct.

**Display math (`$$...$$`):**
- Lines 213–221 (Sec. 2.6): Three consecutive display equations with no blank lines between them. Pandoc/xelatex will render these as three separate display blocks with no vertical gap separator, which may look slightly crowded but is syntactically correct and will compile.
- Lines 561–575 (Sec. 7.1): Correct multi-line display equations with blank-line separation. No issues.
- Lines 544–548 (Sec. 6.8), 761–765, 923–929 (Sec. 9.1, 11): `\boxed{...}` spanning multiple lines — these are valid amsmath constructs and will render correctly with xelatex.
- Lines 704–707 (Sec. 8.2): Display equation block with `\approx` — correct.
- Lines 727–729: Single-line display with `\quad` — correct.

**Potential rendering concern — `\text{bad tag}` (line 562):** The subscript `N_\text{bad tag}` contains a space inside `\text{}`. This is legal LaTeX and will render "bad tag" with a space, which is the intended appearance.

**`\texttt{...}` in display math (line 214):** `\texttt{wc -l events.csv}` inside `$$...$$` is correct LaTeX but requires the `fontenc`/`lmodern` or equivalent font package. With XeLaTeX this is standard and will render correctly.

**`$\sigma$(Std)` in table header (line 977):** The expression `$\sigma$(Std)` is valid inline math for `σ` followed by literal `(Std)`. This will render as "σ(Std)" — the parenthesized text will be upright roman, which is the apparent intent. No issue.

**`$^*$` in table cell (line 750):** `200$^*$` is valid and will render the asterisk as a math superscript. Correct.

**`$\to$` in table cells (lines 1017–1024, Appendix C):** Valid math mode arrows. No issue.

---

## Layout

**Section numbering:** Sections are manually numbered in headings (e.g., `# 1. Introduction`, `## 1.1 Physics Motivation`) AND also carry `{#sec:...}` labels. If pandoc is invoked with `--number-sections`, headings will receive automatic numbers, producing double numbering (e.g., "1. 1. Introduction"). This is a **significant layout risk** depending on the pandoc build command. Source inspection cannot determine whether `--number-sections` is in the build command.

**Tilde (`~`) as non-breaking space:** Used in several places outside math mode:
- Line 26: `$\langle E \rangle = ...$~GeV` — tilde immediately after a closing `$` is a non-breaking space before "GeV". Pandoc treats `~` as a non-breaking space (via `\~{}` in LaTeX output). This will render correctly as "... GeV" with non-breaking space.
- Line 166–167: `$E = 22.4$~GeV` — same pattern, correct.
- Line 265: `Abs.~Eff.~(\%)` and `Rel.~Eff.~(\%)` inside a table cell — tildes will render as non-breaking spaces, giving "Abs. Eff. (%)" which is the intended result. Correct.
- Line 299: `$12.1$--$29.4$~GeV` — correct.

**Escaped percent signs (`\%`) in markdown text:** Used extensively (e.g., `0.5\%`, `47.4\%`). In pandoc markdown, `\%` is a literal `%` (the backslash has no special role in markdown outside math). This will render correctly in the PDF as `%` since pandoc passes it through to LaTeX as `\%`, which is a literal percent. No issue.

**`excl.\ header` (line 267):** The `\ ` (backslash-space) is a LaTeX inter-sentence space suppressor. In pandoc markdown this becomes `\ ` in the LaTeX output, rendering as a normal inter-word space. Correct intended use.

**`vs.\` (line 879):** Same pattern, correct.

**`10\%` in section headings (line 699, 1038):** Heading `## 8.2 10\% Subsample Diagnostic` — the `\%` in a heading will render as `%`. Correct.

**Wide tables — potential margin overflow (Category B):**
- `tbl:full-stats` (Appendix A, line 977–985): 13 columns. This table is very wide and is very likely to overflow the text width even in landscape or with font size reduction. Without a `longtable` or `\resizebox` wrapper, xelatex will likely overflow the right margin.
- `tbl:subsample` (line 711): 6 columns with long column headers (e.g., `$|\Delta|/\text{SE}_\text{sub}$`) — borderline but may fit.
- `tbl:kinematics` (line 788–796): 5 columns but cells contain complex expressions and unit strings — may be tight.

**`--` em-dash usage:** `$12.1$--$29.4$` (line 299) uses `--` between two math expressions. In pandoc, `--` in running text becomes an en-dash. Between math spans the behaviour may be unpredictable — the `--` is in markdown text (between two closing/opening `$`), so it will render as an en-dash. This is likely the intended result (a range).

**`::: {#refs} :::` block (lines 1054–1055):** This is the correct pandoc-citeproc reference list div. It will cause the bibliography to be placed at this location. Correct placement under the `# References` heading.

---

## Cross-References

**Total `@tbl:` references found**: 14
**Total `@sec:` references found**: 4
**`@fig:` references**: 0 (no figures in document)
**`@eq:` references**: 0 (no numbered equations)

**All referenced `@tbl:` labels and their definitions:**

| Reference used | Label defined? |
|----------------|---------------|
| `@tbl:schema` (line 122) | Yes (line 137) |
| `@tbl:row-pathologies` (line 145) | Yes (line 154) |
| `@tbl:cutflow` (lines 263, 272) | Yes (line 272) |
| `@tbl:encoding` (line 889) | Yes (line 203) |
| `@tbl:syst-overview` (line 360) | Yes (line 376) |
| `@tbl:syst-dedup-means` (line 594) | Yes (line 459) |
| `@tbl:syst-null` (line 735) | Yes (line 483) |
| `@tbl:yield` (line 769) | Yes (line 781) |

**All `@sec:` cross-references and their definitions:**

| Reference used | Label defined? |
|----------------|---------------|
| `@sec:cutflow` (lines 161, 317) | Yes (line 261) |
| `@sec:syst-dedup` (lines 171, 278) | Yes (line 417) |

No unresolved cross-references detected at source level. All `@tbl:` and `@sec:` targets have matching `{#tbl:...}` or `{#sec:...}` definitions.

- **Total references found**: ~14 table + 4 section = ~18
- **Resolved**: 18
- **Unresolved**: None

---

## Citations

**Citations found in text:**
- `[@ALEPH:2005ab]` — lines 54, 351
- `[@ALEPH:2005ab; @PDG2024]` — line 83
- `[@PDG2024]` — line 255
- `[@EfronTibshirani1993]` — line 578

**Citation keys used**: `ALEPH:2005ab`, `PDG2024`, `EfronTibshirani1993`

**Bibliography entries present in the embedded BibTeX block** (lines 1059–1091):
- `ALEPH:2005ab` — present
- `PDG2024` — present
- `EfronTibshirani1993` — present

**Uncited bibliography entries**: None.

**CRITICAL ISSUE — BibTeX embedded in a fenced code block (Category A):**
The bibliography is placed at lines 1059–1091 inside a ```` ```bibtex ... ``` ```` fenced code block. Pandoc-citeproc does **not** read BibTeX from fenced code blocks embedded in the markdown source. It requires an external `.bib` file referenced via the YAML front matter key `bibliography:` or via the `--bibliography` command-line flag. As the document stands:
1. There is no `references.bib` (or any `.bib`) file in the analysis directory.
2. The YAML front matter (lines 1–5) has no `bibliography:` key.
3. All four citation keys (`ALEPH:2005ab`, `PDG2024`, `EfronTibshirani1993`) will be **unresolved** at compile time.
4. The compiled PDF will show `[@ALEPH:2005ab]`, `[@PDG2024]`, and `[@EfronTibshirani1993]` as literal raw text, or pandoc-citeproc will emit warnings and leave them unprocessed.
5. The `# References` section will be empty (the `::: {#refs} :::` div will produce no content).

**Total citations found**: 5 citation instances (3 unique keys)
**Resolved**: 0 (all unresolved at compile time due to missing .bib file)
**Unresolved**: `ALEPH:2005ab`, `PDG2024`, `EfronTibshirani1993`
**Bibliography entries in .bib file**: 0 (no .bib file exists)

---

## Table Formatting

All 34 tables use the correct pandoc pipe-table syntax (`| col | col |` with `|---|---|` header separators). Caption syntax (`: Caption text. {#tbl:label}`) is correct pandoc table caption format for all tables.

**Specific table assessments:**

- `tbl:dataset-properties` through `tbl:encoding` (Sec. 2): Simple 2–4 column tables. Well-formed. No overflow risk.
- `tbl:event-def`, `tbl:cutflow` (Sec. 3–4): 3–5 columns. Fine.
- `tbl:kinematics` (line 788, Sec. 9.2): 5 columns with long cell content. The header row is borderline. Cells like `Std $\pm \sigma(\text{std})$` and `Median [95\% CI]` are lengthy. May be tight but likely fits within standard margins.
- `tbl:subsample` (line 711, Sec. 8.2): 6 columns; the last header `$|\Delta|/\text{SE}_\text{sub}$` is moderately wide. Likely fits.
- `tbl:full-stats` (line 977, Appendix A): **13 columns** — this table will almost certainly overflow the right margin without a scaling directive. The column headers alone span a very wide row. This is a Category B issue.
- `tbl:sensitivity` (line 1015, Appendix C): 5 columns with a long first column ("Parameter variation"). Content is long but manageable for 5 columns.
- `tbl:syst-overview` (line 362, Sec. 6.1): 4 columns with long cells in columns 3 and 4. May be tight.
- `tbl:4a-full` (line 996, Appendix B): 6 columns with moderately wide headers. Borderline.

**Column alignment:** Pandoc infers alignment from the separator dashes. Most tables use left alignment throughout (no colons in separator rows). For numerical tables, right-alignment would be more conventional but this is a Category C style preference, not a rendering error.

---

## Page Count Assessment
- **Total pages**: Not measured (no compiled PDF)
- **Estimated**: ~25–35 pages based on document length (~1092 lines with 34 tables and extensive text)
- **Assessment**: Potentially too short for a complete analysis note (target 50–100 pages), but the document covers all required sections

---

## Issues

### Category A (Blocking)

**A-1: Missing `references.bib` — all citations will be unresolved.**
The BibTeX entries for `ALEPH:2005ab`, `PDG2024`, and `EfronTibshirani1993` are embedded in a ```` ```bibtex ``` ```` fenced code block (lines 1059–1091) rather than in an external `.bib` file. Pandoc-citeproc cannot read bibliography data from fenced code blocks. The YAML front matter lacks a `bibliography:` key, and no `.bib` file exists in the analysis directory. At compile time, all `[@key]` citations will fail to resolve, appearing as raw text in the output, and the `# References` section will be empty.

**Fix:** Extract the BibTeX content from the fenced block into a file `references.bib` in the same directory, and add `bibliography: references.bib` to the YAML front matter.

---

### Category B (Important)

**B-1: 13-column table `tbl:full-stats` (Appendix A, line 977) will likely overflow page margins.**
The table has 13 columns: Variable, N, Mean, SE, Std, σ(Std), Median, CI_low, CI_high, Min, Max, p_1, p_99. With standard A4/letter margins this table cannot fit within the text width. Without a `\resizebox`, `adjustbox`, or `longtable` / `tabularx` environment wrapper, xelatex will overflow the right margin or produce a "float too large for page" error.

**Fix:** Add a pandoc raw LaTeX block around the table with `\resizebox{\textwidth}{!}{...}` or convert it to a `longtable` with column-width specifications.

**B-2: Double section numbering risk (all numbered headings).**
Section headings use explicit manual numbers in the heading text (`# 1. Introduction`, `## 1.1 Physics Motivation`, etc.) as well as `{#sec:...}` anchors. If the pandoc build command includes `--number-sections` (common for analysis notes), pandoc will prepend automatic numbers, yielding "1. 1. Introduction", "1.1. 1.1 Physics Motivation", etc., throughout the document. This affects every heading in the document (30+ headings).

**Fix:** Either remove the explicit manual numbering from headings and rely on `--number-sections`, or ensure the build command does NOT use `--number-sections`.

**B-3: Three consecutive display math blocks without separator text (lines 213–221, Sec. 2.6).**
The three `$$...$$` blocks for the line-count cross-check are stacked with no blank line between them:
```
$$
\texttt{wc -l events.csv} = 202 \text{ lines}
$$
$$
N_\text{non-data} = 1 \text{ (header)}
$$
$$
N_\text{data} = 202 - 1 = 201
$$
```
Pandoc requires a blank line between block-level elements. Without blank lines separating the `$$` blocks, pandoc may interpret the sequence ambiguously — some versions will merge adjacent `$$` blocks or misparse the boundaries. The safe form requires a blank line after each closing `$$`.

**Fix:** Add a blank line after the closing `$$` of each display block.

**B-4: ```` ```bibtex ``` ```` code block at end of document will render as a visible code listing.**
Even if a `.bib` file is created (fixing A-1), the fenced `bibtex` code block (lines 1059–1091) will still appear in the compiled PDF as a verbatim code listing under the References section, showing all raw BibTeX text. This is not the intended output.

**Fix:** Remove the ```` ```bibtex ... ``` ```` code block entirely once `references.bib` is created.

---

### Category C (Minor)

**C-1: No `geometry` or `fontsize` YAML keys in front matter.**
The YAML front matter contains only `title`, `author`, and `date`. Wide tables (especially `tbl:full-stats`) would benefit from explicit margin configuration such as `geometry: margin=2cm` or `fontsize: 10pt` to give more horizontal space.

**C-2: Numerical columns in tables are left-aligned rather than right-aligned.**
Pandoc pipe tables without explicit alignment colons default to left alignment. Numerical columns (counts, percentages, uncertainties) are conventionally right-aligned in physics publications. This is a style preference rather than a rendering error.

**C-3: Estimated page count (~25–35 pages) is below the 50–100 page target for a complete analysis note.**
The note is thorough in coverage but the page count may be short of the ideal range depending on table formatting and font choices. This is a minor concern and does not affect rendering correctness.

---

## Recommendations

The following must be resolved before the document is rendering-ready:

1. **(Blocking)** Create `/Users/bamboo/Githubs/ClawHep/MoltHep/analyses/count-events-in-a-simple-20260326/references.bib` containing the three BibTeX entries currently embedded in the fenced code block. Add `bibliography: references.bib` to the YAML front matter. Remove the ```` ```bibtex ``` ```` fenced block from the end of the document.

2. **(Important)** Add blank lines between the three consecutive `$$...$$` blocks at lines 213–221 to ensure correct pandoc parsing.

3. **(Important)** Address the 13-column `tbl:full-stats` table overflow by wrapping it in a resize or adjustbox directive, or splitting it into two narrower tables.

4. **(Important)** Confirm whether the pandoc build command uses `--number-sections`. If it does, remove the explicit `1.`, `1.1`, etc., numbers from all section headings. If it does not, the current manual numbering is acceptable.
