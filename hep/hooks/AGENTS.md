<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Analysis Directory Isolation Hook

## Purpose

This directory contains the `isolate.sh` bash hook that enforces analysis directory isolation. The hook blocks file access outside the active analysis run directory, preventing agents from accidentally modifying files in the HEP methodology, conventions, or other analyses. It allows intentional symlinks (e.g., conventions/ symlink) while strictly controlling agent file I/O.

## Key Files

| File | Purpose |
|------|---------|
| isolate.sh | Bash hook that intercepts all file-access tool calls and permits/denies based on analysis directory boundary |

## Isolation Rules

The hook enforces these rules:

1. **Development mode:** If no `.analysis_config` file exists in the current directory ancestry, all file access is allowed (dev environment without active analysis)

2. **Active analysis mode:** If `.analysis_config` exists, file access is restricted:
   - **Allowed:** Current analysis directory, `/tmp`, `data_dir` (from config), any `allow=` paths (from config)
   - **Allowed:** Symlinks within the analysis directory (e.g., `conventions/` symlink to shared conventions)
   - **Blocked:** Any file access outside the allowed paths

3. **Path resolution:** The hook checks both logical paths (before symlink resolution, to allow conventions/ symlink) and physical paths (after symlink resolution)

4. **Tools checked:** All file-access tools (Read, Write, Edit, Bash) are subject to the hook. No file path → no restriction.

## Configuration

Each analysis directory must have `.analysis_config` at the root:

```yaml
# .analysis_config

# Required: location of input data files
data_dir=/path/to/data

# Optional: additional allowed paths
allow=/another/path
allow=/yet/another/path
```

## Hook Integration

The hook is invoked before any tool use by the Claude Code harness:

```
Agent → Tool Call → isolate.sh Hook → [DENY/ALLOW] → Tool Execution
```

If the hook denies access, the tool call fails with a permission error (not a tool error).

## For AI Agents

### Working In This Directory

1. **The hook is automatic:** Agents do not invoke the hook directly. It runs as a pre-check before every file-access tool call in Claude Code.

2. **Development mode is transparent:** When working in the repo root without an active analysis, the hook allows all file access. This enables documentation editing and testing.

3. **Active analysis mode is strict:** When `.analysis_config` is present, agents are confined to the analysis directory. This prevents accidental modifications to shared conventions or methodology.

4. **Intentional symlinks work:** The hook allows symlinks within the analysis directory (e.g., `conventions/ → /Users/bamboo/Githubs/Mol-HEP-Lab/hep/conventions`). Symlinks enable agents to read shared conventions without copying files.

5. **Custom data paths:** If your analysis needs access to data outside the analysis directory, set `data_dir` or `allow=` in `.analysis_config`.

### Testing Requirements

- The hook must allow file access in development mode (repo root without `.analysis_config`)
- The hook must deny file access outside the analysis directory when `.analysis_config` is present
- The hook must allow access to `data_dir` and `allow=` paths configured in `.analysis_config`
- The hook must allow symlinks within the analysis directory
- The hook must work with both relative and absolute file paths
- Error messages must be clear about why access was denied

### Common Patterns

- **Conventions symlink:** `analyses/{analysis_name}/conventions/ → /Users/bamboo/Githubs/Mol-HEP-Lab/hep/conventions`
- **Data directory:** Set `data_dir=/mnt/data` in `.analysis_config` if ROOT files are in `/mnt/data`
- **Development mode:** Edit documentation in repo root without `.analysis_config`; the hook allows all access
- **Error recovery:** If denied access, check that the file path is within the allowed directories (analysis_dir, data_dir, allow= paths)

## Implementation Details

The hook:

1. Reads JSON input from Claude Code harness (tool_name, cwd, file_path)
2. Checks for `.analysis_config` in current directory ancestry (finds analysis root)
3. Builds list of allowed paths from config (analysis_dir, /tmp, data_dir, allow= entries)
4. Tests logical path (before symlink resolution, to allow conventions/ symlink)
5. Tests physical path (after symlink resolution, for absolute safety)
6. Returns JSON decision (allow or deny with reason)

## Dependencies

### Internal

- Enforces the isolation boundary defined in `../CLAUDE.md` ("Agents **never modify** files outside the active analysis run directory")
- Works with analysis structure defined in `../orchestration/sessions.md`
- Allows read-only access to conventions defined in `../conventions/`

### External

- Claude Code harness: Invokes the hook before tool execution
- Bash: Hook is a bash script
- jq: JSON parsing/generation
- realpath: Path normalization (available on Linux/macOS)
