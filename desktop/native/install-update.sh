#!/bin/sh
# $1 installed app, $2 verified staged app, $3 rollback app, $4 current PID.
set -eu
while kill -0 "$4" 2>/dev/null; do sleep 1; done
if /bin/mv "$1" "$3"; then
  if /bin/mv "$2" "$1"; then
    /usr/bin/open "$1" || true
    # Keep rollback material for recovery; the next successful launch can be inspected.
  else
    /bin/mv "$3" "$1"
    /usr/bin/open "$1" || true
  fi
fi
