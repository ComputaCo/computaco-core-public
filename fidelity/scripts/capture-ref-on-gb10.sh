#!/usr/bin/env bash
# Capture a reference trajectory on gb10-direct (the idle batch box; AGENTS.md § The owner's
# machine) and bring the store back. gb10 has no Rust toolchain: the binary built here is
# copied over (both machines are aarch64 Linux).
#
#   fidelity/scripts/capture-ref-on-gb10.sh <procedure.json> --app kicad [cwfid capture-ref flags]
#
# Paths in the flags are on gb10: the templates and target maps are synced to
# $REMOTE/fidelity/{templates,targets}, so pass e.g.
#   --home-template /home/<user>/cwfid/fidelity/templates/kicad-home --target-map /home/<user>/cwfid/fidelity/targets/kicad.json
#
# The store on gb10 is ~/cwfid/store; it is rsynced back into $CWFID_STORE (default
# ./fidelity-store) after the run, so trajectory ids and blobs merge by content.
set -euo pipefail
HOST=${GB10_HOST:-gb10-direct}
REMOTE=${GB10_DIR:-'~/cwfid'}
STORE=${CWFID_STORE:-fidelity-store}
PROCEDURE=$1; shift
BIN=target/debug/cwfid
[ -x "$BIN" ] || { echo "build first: flock ~/.claude/jobs/cargo.lock cargo build -p cw-fidelity-tool" >&2; exit 1; }
WORLD_DIR=$(dirname "$(dirname "$PROCEDURE")")
ssh "$HOST" "mkdir -p $REMOTE/bin $REMOTE/store $REMOTE/worlds $REMOTE/fidelity"
rsync -az "$BIN" "$HOST:$REMOTE/bin/cwfid"
rsync -az fidelity/tool/py/atspi_dump.py "$HOST:$REMOTE/bin/atspi_dump.py"
rsync -az --delete fidelity/templates/ "$HOST:$REMOTE/fidelity/templates/"
rsync -az --delete fidelity/targets/ "$HOST:$REMOTE/fidelity/targets/"
rsync -az --delete --exclude 'computers/*/root/**' "$WORLD_DIR/" "$HOST:$REMOTE/worlds/$(basename "$WORLD_DIR")/"
REMOTE_PROC="$REMOTE/worlds/$(basename "$WORLD_DIR")/procedures/$(basename "$PROCEDURE")"
REMOTE_WORLD="$REMOTE/worlds/$(basename "$WORLD_DIR")/world.json"
# shellcheck disable=SC2029
ssh "$HOST" "cd $REMOTE && CWFID_ATSPI_HELPER=$REMOTE/bin/atspi_dump.py ./bin/cwfid capture-ref $REMOTE_PROC --world $REMOTE_WORLD --store $REMOTE/store $(printf '%q ' "$@")"
mkdir -p "$STORE"
rsync -az "$HOST:$REMOTE/store/" "$STORE/"
echo "store synced to $STORE"
