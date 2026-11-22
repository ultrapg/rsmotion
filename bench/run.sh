#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-help}"

case "$MODE" in
  baseline)
    cd "$ROOT_DIR/tests/fixtures"
    exec node scripts/render-baseline.mjs
    ;;
  spike-encode)
    shift
    exec "$ROOT_DIR/target/release/rsmotion" spike-encode "$@"
    ;;
  spike-raster)
    shift
    exec "$ROOT_DIR/target/release/rsmotion" spike-raster "$@"
    ;;
  benchmark)
    shift
    exec "$ROOT_DIR/target/release/rsmotion" benchmark "$@"
    ;;
  *)
    echo "Usage: $0 {baseline|spike-encode|spike-raster|benchmark} [args...]"
    exit 1
    ;;
esac
