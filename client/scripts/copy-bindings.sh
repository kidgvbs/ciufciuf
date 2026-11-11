#!/usr/bin/env bash
# Copia i binding ts-rs da ttr-proto a client/src/lib/proto/
set -euo pipefail

SRC="$(dirname "$0")/../../crates/ttr-proto/bindings"
DST="$(dirname "$0")/../src/lib/proto"

mkdir -p "$DST"
# Rimuove i binding non più generati; index.ts è scritto a mano.
for f in "$DST"/*.ts; do
  name="$(basename "$f")"
  if [ "$name" != "index.ts" ] && [ ! -e "$SRC/$name" ]; then rm "$f"; fi
done
cp "$SRC"/*.ts "$DST"/
echo "Copied $(ls "$DST"/*.ts | wc -l) binding files to src/lib/proto/"