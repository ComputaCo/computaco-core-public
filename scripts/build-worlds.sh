#!/usr/bin/env bash
# Regenerate each open world's world.json from its blueprint (world.yml: includes, computers, services and the
# `place:` blocks that derive nodes, links and DNS records), and check the node-app packages against the catalog.
set -euo pipefail
cd "$(dirname "$0")/.."
for world in worlds/*/world.yml; do
  cargo run --quiet -p cw-blueprint --bin cw-world -- build "$world"
done
node scripts/pack-node-apps.mjs target/node-apps >/dev/null
