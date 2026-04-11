<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Source Code Root

## Purpose
TypeScript source code for the React frontend. Includes app component, state management, WebSocket communication, type definitions, internationalization, mock data, styling, and component library.

## Key Files
| File | Description |
|------|-------------|
| `App.tsx` | Root component: app state reducer, WebSocket lifecycle, agent/resource dispatchers, layer filtering, dark/light theme toggle, locale context setup |
| `main.tsx` | Entry point: mounts App to DOM with React StrictMode |
| `types.ts` | Core types: agents, artifacts, stages, layers, messages, resource stats; 18-stage pipeline definitions aligned with backend |
| `mock.ts` | Mock message generator: creates synthetic agent/artifact updates for testing without backend |
| `App.css` | Styling: pyramid layout, layer panels, cards, animations, theme variables |
| `index.css` | Global styles: fonts, colors, grid system, dark/light theme CSS custom properties |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `components/` | React components for UI panels and interactive elements |
| `i18n/` | Internationalization: locale context, translation strings (English/Chinese) |
| `assets/` | Static assets: logos, icons, images |

## For AI Agents

### Working In This Directory

All source files are in TypeScript. Type-check with `npm run build` before committing.

Add new components in `components/` directory. Use `memo()` for large components to prevent unnecessary re-renders.

**Important:** Keep component and type definitions in sync. Check `types.ts` for `MolAgent`, `Artifact`, `LogEntry` before rendering.

### Testing Requirements

- Run `npm run build` to catch TypeScript errors.
- Test components in mock mode by inspecting `src/mock.ts` and `INITIAL_AGENTS` structure.
- Verify i18n keys exist in both `i18n/en.ts` and `i18n/zh.ts`.
- CSS updates should use CSS custom properties (--layer-color, --error-color, etc.) for theme consistency.
- WebSocket message types must match union type definitions in `types.ts`.

### Common Patterns

**WebSocket message dispatch pattern:**
```tsx
dispatch({ type: 'agent_update', payload: agent });
dispatch({ type: 'artifact_produced', payload: artifact });
dispatch({ type: 'log', payload: logEntry });
```

**Stage metadata lookup:**
```tsx
const meta = STAGE_META[stageId]; // { id, phase, step, name, key, outputs }
```

**Agent filtering by layer:**
```tsx
const explorationAgents = agents.filter(a => a.layer === AgentLayer.EXPLORATION);
```

**CSS theme variables:**
```css
var(--layer-color) /* Set per-layer in LayerPanel */
var(--text-primary) /* Dark theme: white, Light theme: black */
var(--bg-primary) /* Dark theme: #1a1a1a, Light theme: #ffffff */
```

## Dependencies
### Internal
- `components/` — UI components
- `i18n/` — translation system
- `types.ts` — data types

### External
- React 19.2.4
- Vite 8.0.8 (dev/build)
- TypeScript 5.9.3
