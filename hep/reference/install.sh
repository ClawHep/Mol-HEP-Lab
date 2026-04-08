#!/usr/bin/env bash
# MoltHep Installer — Digital Physicist
# Usage: curl -fsSL https://raw.githubusercontent.com/ClawHep/MoltHep/main/install.sh | bash
set -euo pipefail

# --- Colors ---
BLUE='\033[0;34m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

info()  { echo -e "${BLUE}==>${NC} $*"; }
ok()    { echo -e "${GREEN}==>${NC} $*"; }
warn()  { echo -e "${YELLOW}==>${NC} $*"; }
err()   { echo -e "${RED}==>${NC} $*" >&2; }
step()  { echo -e "\n${BOLD}[$1/$TOTAL_STEPS]${NC} $2"; }

TOTAL_STEPS=6
REPO_URL="https://github.com/ClawHep/MoltHep.git"
INSTALL_DIR="${MOLTHEP_HOME:-$HOME/MoltHep}"
BIN_DIR="${HOME}/.local/bin"

# --- Banner ---
echo ""
echo -e "${BOLD}  MoltHep Installer${NC}"
echo -e "${DIM}  Digital Physicist — 24/7 Autonomous HEP Research Agent${NC}"
echo ""

# --- Check prerequisites ---
step 1 "Checking prerequisites..."

_have() { command -v "$1" >/dev/null 2>&1; }

MISSING=()
_have git     || MISSING+=("git")
_have cargo   || MISSING+=("rust (install via https://rustup.rs)")
_have node    || MISSING+=("node >= 20 (install via https://nodejs.org)")
_have python3 || MISSING+=("python >= 3.10")

if [ ${#MISSING[@]} -gt 0 ]; then
  err "Missing required tools:"
  for m in "${MISSING[@]}"; do
    echo -e "  ${RED}-${NC} $m"
  done
  echo ""
  err "Install the missing tools and re-run this script."
  exit 1
fi

# Check versions
RUST_VER=$(rustc --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "0.0")
NODE_VER=$(node --version 2>/dev/null | grep -oE '[0-9]+' | head -1 || echo "0")
PY_VER=$(python3 --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "0.0")

ok "git $(git --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
ok "rust $RUST_VER"
ok "node v$NODE_VER"
ok "python $PY_VER"

# --- Clone or update ---
step 2 "Setting up project..."

if [ -f "$INSTALL_DIR/molthep.toml" ]; then
  info "Found existing installation at $INSTALL_DIR"
  cd "$INSTALL_DIR"
  git pull --rebase 2>/dev/null || git pull || true
else
  info "Cloning MoltHep to $INSTALL_DIR..."
  git clone "$REPO_URL" "$INSTALL_DIR"
  cd "$INSTALL_DIR"
fi

# --- Build Web UI ---
step 3 "Building Web UI..."

if [ -d "zeroclaw/web" ]; then
  cd zeroclaw/web
  npm install --silent 2>&1 | tail -1
  npm run build 2>&1 | tail -1
  cd ../..
  ok "Web UI built"
else
  warn "Web UI directory not found, skipping"
fi

# --- Build Rust daemon ---
step 4 "Building daemon (this may take a few minutes)..."

cd zeroclaw
cargo build --release 2>&1 | tail -5
cd ..

if [ -x "zeroclaw/target/release/zeroclaw" ]; then
  ok "Daemon built successfully"
else
  err "Daemon build failed"
  exit 1
fi

# Ensure bin/ symlink
mkdir -p bin
ln -sf ../zeroclaw/target/release/zeroclaw bin/zeroclaw

# --- Install molthep command globally ---
step 5 "Installing molthep command..."

mkdir -p "$BIN_DIR"

# Copy wrapper script with MOLTHEP_HOME baked in
sed "1,/^PROJECT_ROOT=/ {
  /^  PROJECT_ROOT=.*cd.*pwd/ {
    i\\
  PROJECT_ROOT=\"$INSTALL_DIR\"
  }
}" bin/molthep > "$BIN_DIR/molthep" 2>/dev/null || cp bin/molthep "$BIN_DIR/molthep"

# Simpler approach: just copy and set env
cp bin/molthep "$BIN_DIR/molthep"
chmod +x "$BIN_DIR/molthep"

# Add to shell config if not already there
SHELL_RC=""
if [ -f "$HOME/.zshrc" ]; then
  SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
  SHELL_RC="$HOME/.bashrc"
fi

NEED_PATH=false
NEED_HOME=false

if [ -n "$SHELL_RC" ]; then
  grep -q 'MOLTHEP_HOME' "$SHELL_RC" 2>/dev/null || NEED_HOME=true
  grep -q '\.local/bin' "$SHELL_RC" 2>/dev/null || NEED_PATH=true

  if $NEED_HOME || $NEED_PATH; then
    echo "" >> "$SHELL_RC"
    echo "# MoltHep — Digital Physicist" >> "$SHELL_RC"
  fi
  if $NEED_HOME; then
    echo "export MOLTHEP_HOME=\"$INSTALL_DIR\"" >> "$SHELL_RC"
  fi
  if $NEED_PATH; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_RC"
  fi

  if $NEED_HOME || $NEED_PATH; then
    ok "Added MOLTHEP_HOME and PATH to $SHELL_RC"
  fi
fi

export MOLTHEP_HOME="$INSTALL_DIR"
export PATH="$BIN_DIR:$PATH"

# --- Verify ---
step 6 "Verifying installation..."

echo ""
"$BIN_DIR/molthep" --version
echo ""

# Check Python module
if PYTHONPATH="$INSTALL_DIR/src" python3 -c "import molthep" 2>/dev/null; then
  ok "Python orchestration module OK"
else
  warn "Python module not importable (run: PYTHONPATH=$INSTALL_DIR/src python3 -c 'import molthep')"
fi

# Check if an executor is available
if _have claude; then
  ok "Executor found: claude (Claude Code CLI)"
elif _have codex; then
  ok "Executor found: codex"
elif _have ollama; then
  ok "Executor found: ollama"
else
  warn "No AI executor found. Install at least one: claude, codex, or ollama"
fi

# --- Source shell rc to make molthep available immediately ---
if [ -n "$SHELL_RC" ]; then
  # shellcheck disable=SC1090
  source "$SHELL_RC" 2>/dev/null || true
fi

# --- Done ---
echo ""
echo -e "${GREEN}${BOLD}Installation complete!${NC}"
echo ""

# --- Auto-launch onboard ---
if [ -x "$BIN_DIR/molthep" ] && [ -n "$ZEROCLAW" ]; then
  echo -e "${BLUE}Launching initial configuration...${NC}"
  echo ""
  "$BIN_DIR/molthep" onboard || true
  echo ""
  echo -e "${GREEN}${BOLD}You're all set!${NC}"
  echo ""
  echo -e "  ${BOLD}Get started:${NC}"
  echo ""
  echo -e "  ${DIM}$${NC} ${BOLD}molthep new${NC}              Create your first analysis"
  echo -e "  ${DIM}$${NC} ${BOLD}molthep daemon${NC}           Start 24/7 background service"
  echo -e "  ${DIM}$${NC} ${BOLD}molthep${NC}                  Show all commands"
  echo ""
else
  echo -e "  ${BOLD}Get started:${NC}"
  echo ""
  echo -e "  ${DIM}$${NC} ${BOLD}molthep onboard${NC}          Configure LLM providers"
  echo -e "  ${DIM}$${NC} ${BOLD}molthep new${NC}              Create your first analysis"
  echo -e "  ${DIM}$${NC} ${BOLD}molthep daemon${NC}           Start 24/7 daemon"
  echo -e "  ${DIM}$${NC} ${BOLD}molthep${NC}                  Show all commands"
  echo ""
fi

echo -e "${DIM}  Project: $INSTALL_DIR${NC}"
echo -e "${DIM}  Web UI:  http://localhost:42617${NC}"
echo ""
