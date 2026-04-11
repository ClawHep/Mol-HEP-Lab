<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Assets

## Purpose

This directory contains visual and media assets for the Mol-HEP-Lab project: logos, UI screenshots, and a showcase directory of completed research projects demonstrating the platform's capabilities.

## Key Files

| File | Description |
|------|-------------|
| `logo.png` | Primary project logo. Use for documentation, GitHub repository header, and official communications. |
| `logo_v1.png` | Alternative logo design (version 1). Legacy or variation. |
| `logo_v2.png` | Alternative logo design (version 2). Latest iteration. |
| `ui.png` | Screenshot of the React dashboard. Shows the 5-phase pipeline visualization, agent status panel, real-time resource monitor, and artifact browser. |
| `Group.png` | Group diagram or reference image (specific purpose TBD). |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `showcase/` | Completed research project artifacts. Demonstrates real-world usage of the platform. Contains project directories and markdown summaries. |

## Showcase Projects

| Project | Files | Description |
|---------|-------|-------------|
| `adaptive-mrsa` | directory + `showcase-adaptive-mrsa.md` | Adaptive multi-rate Spatial Attention network research project. |
| `droput-cross-domain` | directory + `showcase-dropout-cross-domain.md` | Dropout mechanism cross-domain generalization study. |
| `flowedit-wan21-t2v` | directory + `showcase-flowedit-wan21-t2v.md` | Flow editing for text-to-video generation (WAN21). |
| `quantifying-hallucination` | directory + `showcase-quantify-hallucination.md` | Quantifying and mitigating hallucinations in generative models. |
| `reproduce-phycustom` | directory + `showcase-reproduce-phycustom-on-flux.md` | Reproducing physics-custom models on Flux architecture. |

Additional files:
- `consensus_synthesis.md` — Consensus-based synthesis methodology
- `discussion_transcript.md` — Discussion log or conversation transcript

## For AI Agents

### Using Logos

1. **Documentation**: Use `logo.png` or `logo_v2.png` in README files and blog posts.
2. **Web Assets**: Copy selected logo to `frontend/public/` if needed for the dashboard.
3. **Variant Selection**: `logo_v1.png` and `logo_v2.png` are alternatives; choose one as canonical and deprecate others, or document the purpose of each.

### Screenshot Management

- Keep `ui.png` synchronized with actual dashboard layout
- Update after major frontend changes (new components, layout shifts, color scheme changes)
- Take screenshots on a consistent background (typically light theme or dark theme)
- Store at high resolution (2x or 3x for retina displays) if possible

### Adding Showcase Projects

1. **Create a new project directory**:
   ```
   showcase/my-research-project/
   ├── README.md
   ├── experiment_code.py
   ├── results.json
   ├── figures/
   │   ├── figure1.png
   │   ├── figure2.png
   │   └── ...
   └── paper.md
   ```

2. **Create a summary file**:
   ```
   assets/showcase/showcase-my-research-project.md
   ```
   
   Include:
   - Project title and authors
   - Research question
   - Methodology overview
   - Key results and metrics
   - Link to full project directory
   - Citation (if published)

3. **Document in this AGENTS.md** under the Showcase Projects table

### Content Guidelines

- **Logo**: PNG format, transparent background preferred, square aspect ratio (1:1)
- **Screenshots**: PNG or JPEG, width 1920+ pixels, height 1080+ pixels, aspect ratio 16:9 or wider
- **Showcase markdown**: Include frontmatter with project metadata (title, authors, date, topics, metrics)
- **Figures**: PNG or PDF, high resolution (300 DPI for print), descriptive captions

## For Developers

### Updating Assets

Before committing changes:
```bash
# Check PNG files are valid
file assets/*.png | grep -i "PNG image"

# Check dimensions (optional, requires ImageMagick)
identify assets/*.png
```

### Versioning Logos

If updating logos:
1. Archive old version: `mv logo.png logo_archived_YYYYMMDD.png`
2. Create new version: `logo.png` (canonical)
3. Document in this AGENTS.md what changed and when

## Dependencies

### Internal
- Frontend code may reference `assets/` URLs
- Showcase projects link from `../AGENTS.md` root documentation

### External
- PNG/JPEG image format (standard web formats)
- No external library dependencies (static assets)

## Relationships

- **Root AGENTS.md**: References showcase projects as examples of platform capabilities
- **frontend/**: May pull logos from here via absolute paths
- **docs/**: May embed `ui.png` screenshot in architecture documentation

---

See `../AGENTS.md` for root-level context.
See `showcase/` subdirectories for detailed project documentation.
