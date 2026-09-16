#!/bin/sh
# $1 installed app, $2 verified staged app, $3 rollback app, $4 current PID.
set -eu
[ "$#" -eq 4 ] || exit 1
case "$4" in ''|*[!0-9]*) exit 1 ;; esac
[ -d "$1" ] && [ -d "$2" ] && [ ! -e "$3" ] || exit 1
max_wait=90
elapsed=0
while kill -0 "$4" 2>/dev/null; do
  [ "$elapsed" -lt "$max_wait" ] || exit 1
  sleep 1
  elapsed=$((elapsed + 1))
done
# Both renames occur on the same volume. Never remove the old bundle first.
/bin/mv "$1" "$3" || exit 1
rollback() {
  if [ -e "$1" ]; then /bin/mv "$1" "${2}.failed" || exit 1; fi
  /bin/mv "$3" "$1" || exit 1
  /usr/bin/open -n "$1" --args --fluxion-update-failed || true
  exit 1
}
/bin/mv "$2" "$1" || rollback "$@"
/usr/bin/open -n "$1" || rollback "$@"
# Retain previous.app for manual recovery if the new process later fails to start.
