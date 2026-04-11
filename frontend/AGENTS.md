<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Frontend

## Purpose
React 19 + TypeScript + Vite frontend dashboard. Provides real-time visualization of research agent layers (Strategy, Exploration, Execution, Inference, Documentation) executing a five-phase research pipeline. Connects to backend via WebSocket bridges for live agent updates and resource monitoring. Supports dark/light theme and bilingual UI (English/Chinese). Served by axum static file server.

## Key Files
| File | Description |
|------|-------------|
| `package.json` | React 19.2.4, Vite 8.0.8, TypeScript 5.9.3, ESLint config |
| `vite.config.ts` | Vite build config with WebSocket proxy to agent bridge (8906) and resource monitor (8905) |
| `src/App.tsx` | Root component: app state reducer, dual WebSocket connections (agents/resources), layer/log dispatch, mock mode generator |
| `src/types.ts` | Core types: `MolAgent`, `Artifact`, `LogEntry`, `ProjectInfo`, `ResourceStats`, stage enums (18 stages × 5 phases), layer metadata |
| `src/i18n/index.ts` | i18n context and translation system (English/Chinese) |
| `src/i18n/en.ts` | English translation strings |
| `src/i18n/zh.ts` | Chinese translation strings |
| `src/mock.ts` | Mock message generator for testing without backend |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/components/` | React components: panels for layers, projects, artifacts, logs, feedback, resource monitoring |
| `src/i18n/` | Localization: translation files and context provider |
| `src/assets/` | Static assets (images, etc.) |

## For AI Agents

### Working In This Directory

**Setup:** Run `npm install` in this directory. Requires Node.js 18+.

**Dev server:** `npm run dev` launches Vite on http://localhost:5173 with WebSocket proxies to local agent bridge and resource monitor.

**Build:** `npm run build` runs TypeScript type-check then Vite production build to `dist/`.

**Linting:** `npm run lint` runs ESLint with React hooks and refresh rules.

**Tests:** No test framework configured. Update components and verify in browser or mock mode.

### Testing Requirements

- All UI changes must be tested in mock mode (`toggleMockMode` in App.tsx dispatches `createMockMessageGenerator`).
- Mock mode generates synthetic agent updates without backend connection.
- Component structure must match TypeScript types in `src/types.ts` exactly.
- WebSocket message format must match `WSMessage` union type (defined in types.ts).
- Bilingual strings must be added to both `en.ts` and `zh.ts` translation files.
- Dark/light theme CSS variables defined in `src/App.css` and `src/index.css`.

### Common Patterns

**Locale usage:**
```tsx
import { useLocale } from '../i18n';
const { t, locale } = useLocale();
const text = t('key.subkey');
```

**WebSocket dispatch:**
```tsx
const ws = agentWsRef.current;
if (ws && ws.readyState === WebSocket.OPEN) {
  ws.send(JSON.stringify({ command: 'command_name', param: value }));
}
```

**Component memoization:** Use `memo()` wrapper to prevent re-renders of static panels.

**Stage labels:** `phaseStepLabel(stageId)` returns "Phase.Step" format (e.g., "3.2" for stage 9).

## Dependencies
### Internal
- `src/types.ts` — core data types shared across components
- `src/i18n/` — translation system
- `src/mock.ts` — mock data generator

### External
- `react` ^19.2.4 — UI library
- `react-dom` ^19.2.4 — DOM rendering
- `vite` ^8.0.8 — dev server and build tool
- `typescript` ~5.9.3 — type checking
- `@vitejs/plugin-react` ^6.0.1 — JSX/TypeScript support
