#!/usr/bin/env bash
# Publish an APK as the newest Android release: the APK first, then downloads/android.json, so
# a client never reads a manifest whose file is missing. Keeps the 3 newest APKs (downloads in
# flight finish). This publishes a release artifact; it is not a deploy (scripts/deploy.sh).
#
#   scripts/build-apk.sh   # first
#   RELEASE_TARGET=user@host:/srv/stowarzysz/downloads scripts/release-apk.sh [--min] [--notes "..."]
#
# RELEASE_TARGET is the directory nginx serves as /downloads/ (scp syntax, or a local path).
# --min makes this version the minimum: older installs must update before they can be used
# (for an API change old clients can't follow). Otherwise the previous minimum is kept.
set -euo pipefail

: "${RELEASE_TARGET:?set it to the downloads directory, e.g. user@host:/srv/stowarzysz/downloads}"
root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/crates/app/Cargo.toml" | head -n 1)"
IFS=. read -r major minor patch <<< "$version"
code=$((major * 10000 + minor * 100 + patch))

make_min=false
notes=""
while [ $# -gt 0 ]; do
    case "$1" in
        --min) make_min=true ;;
        --notes) notes="$2"; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

apk="$root/dist/stowarzysz-$version.apk"
[ -f "$apk" ] || { echo "$apk is missing: run scripts/build-apk.sh" >&2; exit 1; }

case "$RELEASE_TARGET" in
    [A-Za-z]:/* | [A-Za-z]:\\*) remote=false; host=""; dir="$RELEASE_TARGET" ;; # a Windows drive path
    *:*) remote=true; host="${RELEASE_TARGET%%:*}"; dir="${RELEASE_TARGET#*:}" ;;
    *) remote=false; host=""; dir="$RELEASE_TARGET" ;;
esac
# The command is built here on purpose (paths are quoted inside it).
# shellcheck disable=SC2029
run() { if $remote; then ssh "$host" "$@"; else bash -c "$*"; fi; }
copy() { if $remote; then scp -q "$1" "$host:$2"; else cp "$1" "$2"; fi; }

previous="$(run "cat '$dir/android.json' 2>/dev/null" || true)"
previous_code="$(printf '%s' "$previous" | sed -n 's/.*"version_code": *\([0-9]*\).*/\1/p')"
if [ -n "$previous_code" ] && [ "$code" -le "$previous_code" ]; then
    echo "version $version ($code) is not newer than the published $previous_code: bump crates/app/Cargo.toml" >&2
    exit 1
fi
min="$(printf '%s' "$previous" | sed -n 's/.*"min_version_code": *\([0-9]*\).*/\1/p')"
$make_min && min="$code"
min="${min:-0}"

sha256="$(sha256sum "$apk" | cut -d ' ' -f 1)"
size="$(wc -c < "$apk" | tr -d ' ')"
name="stowarzysz-$version.apk"
manifest="$(mktemp)"
trap 'rm -f "$manifest"' EXIT
notes_json="$(printf '%s' "$notes" | sed 's/\\/\\\\/g; s/"/\\"/g')"
cat > "$manifest" <<EOF
{
  "version_code": $code,
  "version_name": "$version",
  "url": "/downloads/$name",
  "sha256": "$sha256",
  "size": $size,
  "min_version_code": $min,
  "notes": "$notes_json"
}
EOF

run "mkdir -p '$dir'"
copy "$apk" "$dir/$name.part"
run "mv '$dir/$name.part' '$dir/$name'"
copy "$manifest" "$dir/android.json.part"
run "mv '$dir/android.json.part' '$dir/android.json'"
# Keep the 3 newest APKs.
run "cd '$dir' && ls -1t stowarzysz-*.apk | tail -n +4 | xargs -r rm -f"
echo "published $name (versionCode $code, minimum $min) to $RELEASE_TARGET"
