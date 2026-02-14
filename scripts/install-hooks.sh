#!/bin/bash
set -e

HOOKS_DIR="$(git rev-parse --show-toplevel)/.git/hooks"
SCRIPTS_DIR="$(cd "$(dirname "$0")" && pwd)"

ln -sf "$SCRIPTS_DIR/pre-commit" "$HOOKS_DIR/pre-commit"
ln -sf "$SCRIPTS_DIR/pre-push" "$HOOKS_DIR/pre-push"

chmod +x "$HOOKS_DIR/pre-commit" "$HOOKS_DIR/pre-push"
echo "Git hooks installed."
