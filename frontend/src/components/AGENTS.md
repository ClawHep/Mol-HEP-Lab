<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Components

## Purpose
Reusable React components for the research dashboard UI. Includes panels for displaying agent layers, projects, artifacts, logs, human feedback, resource monitoring, and data flow visualization.

## Key Files
| File | Description |
|------|-------------|
| `App.tsx` (parent) | N/A — root component is in `src/` not here |
| `LayerPanel.tsx` | Displays agents in a research layer (Strategy/Exploration/Execution/Inference/Documentation). Shows layer color, agent count, stage progress bar, expandable agent details. Memoized. |
| `ProjectPanel.tsx` | Left sidebar: project list, project status, quick-submit form (topic + research angles), project controls (resume/pause/restart/delete), discussion mode toggle, artifact browser |
| `DataShelf.tsx` | Displays artifacts by repository with search/filter. Shows artifact name, size, status. Expandable preview for markdown/JSON content. Download button. |
| `LogPanel.tsx` | Displays recent log entries (last 100) filtered by layer. Shows timestamp, agent ID, message, layer color coding. Scrollable. |
| `HumanFeedbackPanel.tsx` | Bottom panel: chat-style human feedback input. Allows targeting feedback to specific layers or broadcast to all. Displays message history. |
| `ResourceMonitor.tsx` | Top-right widget: displays real-time CPU/memory/GPU stats from resource monitor WebSocket. Shows connected status. |
| `DataFlowArrow.tsx` | Visual connector between layer tiers. Shows animation when data is flowing (agents working). |
| `QueuePanel.tsx` | Displays queue status by layer (pending/running/completed counts). Not currently used in App.tsx. |

## Subdirectories
None. All components in single directory.

## For AI Agents

### Working In This Directory

**Component structure:**
- Most components are functional components with `memo()` wrapper.
- Props are TypeScript interfaces defined at top of file.
- All components use `useLocale()` from `i18n` for translated text.
- CSS classes are scoped in `src/App.css` and use theme CSS variables.

**Adding a new component:**
1. Create `src/components/MyComponent.tsx`.
2. Define Props interface with TypeScript types from `types.ts`.
3. Wrap exported component with `memo()` if it's rendering many items or complex subtree.
4. Add i18n keys to both `i18n/en.ts` and `i18n/zh.ts`.
5. Import in `App.tsx` and integrate into state dispatch/render flow.

### Testing Requirements

- Verify component accepts correct props as defined in TypeScript interface.
- Check that all `t()` calls use valid keys from translation files.
- Test CSS media queries for responsive layout (if applicable).
- Verify click handlers dispatch correct WebSocket commands (e.g., `list_agents`, `resume_project`).
- Test dark/light theme by toggling theme and verifying CSS variables apply correctly.

### Common Patterns

**Localization:**
```tsx
import { useLocale } from '../i18n';
const { t, locale } = useLocale();
const text = t('key'); // Fallback to key if translation missing
```

**Filtered lists (memoized):**
```tsx
const filteredAgents = useMemo(() => agents.filter(a => a.status === 'working'), [agents]);
```

**Stage metadata:**
```tsx
const stageMeta = STAGE_META[stageId];
const label = phaseStepLabel(stageId); // e.g., "3.2"
```

**Status icons and colors:**
```tsx
const STATUS_ICON: Record<string, string> = { idle: '🧬', working: '🔬', done: '✅' };
```

**Expandable sections:**
```tsx
const [expanded, setExpanded] = useState(false);
<button onClick={() => setExpanded(!expanded)}>Toggle</button>
{expanded && <DetailsComponent />}
```

## Dependencies
### Internal
- `src/types.ts` — MolAgent, Artifact, LogEntry, stage/layer metadata
- `src/i18n/` — useLocale() hook
- `src/App.css` — component styling
- `src/index.css` — global styles

### External
- `react` — hooks (useState, useMemo, useCallback, memo)
- `react-dom` — DOM utilities
