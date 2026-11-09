#!/usr/bin/env bash
# Copia i binding ts-rs da ttr-proto a client/src/lib/proto/
set -euo pipefail

SRC="$(dirname "$0")/../../crates/ttr-proto/bindings"
DST="$(dirname "$0")/../src/lib/proto"

mkdir -p "$DST"
cp "$SRC"/*.ts "$DST"/
echo "Copied $(ls "$DST"/*.ts | wc -l) binding files to src/lib/proto/"