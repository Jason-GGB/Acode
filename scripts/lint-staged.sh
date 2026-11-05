#!/bin/sh
# Graphical Git starts hooks with a minimal PATH, so fnm's pnpm is invisible.
set -e

if ! command -v pnpm >/dev/null 2>&1; then
  fnm_bin="$HOME/.local/share/fnm/aliases/default/bin"
  if [ -d "$fnm_bin" ]; then
    PATH="$fnm_bin:$PATH"
  fi
  if [ -d /opt/homebrew/bin ]; then
    PATH="/opt/homebrew/bin:$PATH"
  fi
  export PATH
fi

if command -v pnpm >/dev/null 2>&1; then
  exec pnpm lint-staged
fi

if command -v corepack >/dev/null 2>&1; then
  exec corepack pnpm lint-staged
fi

echo "pre-commit: pnpm not found. Install pnpm or add it to PATH." >&2
exit 127
