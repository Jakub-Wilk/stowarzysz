#!/usr/bin/env bash
# Build the signed release APK into dist/stowarzysz-<version>.apk.
#
#   STOWARZYSZ_ORIGIN=https://stowarzysz.example.com \
#   ANDROID_KEYSTORE=~/secrets/stowarzysz.jks ANDROID_KEYSTORE_PASSWORD=... \
#   scripts/build-apk.sh
#
# dx bundles the Gradle project but hard-codes versionCode = 1 and would write signing passwords
# into it, so this sets versionCode itself (major*10000 + minor*100 + patch, as
# config::version_code does), rebuilds with Gradle and signs with apksigner.
# Needs ANDROID_HOME (SDK with build-tools) and ANDROID_NDK_HOME.
set -euo pipefail

: "${STOWARZYSZ_ORIGIN:?set it to the server the app talks to, e.g. https://stowarzysz.example.com}"
: "${ANDROID_KEYSTORE:?set it to the release keystore (.jks)}"
: "${ANDROID_KEYSTORE_PASSWORD:?set it to the keystore password}"
: "${ANDROID_HOME:?set it to the Android SDK}"
key_alias="${ANDROID_KEY_ALIAS:-stowarzysz}"

root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/crates/app"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$app/Cargo.toml" | head -n 1)"
IFS=. read -r major minor patch <<< "$version"
code=$((major * 10000 + minor * 100 + patch))
echo "stowarzysz $version (versionCode $code) for $STOWARZYSZ_ORIGIN"

build_tools="$(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1)"
[ -n "$build_tools" ] || { echo "no build-tools in $ANDROID_HOME" >&2; exit 1; }

export STOWARZYSZ_ORIGIN
(cd "$app" && dx bundle --platform android --release)

gradle_dir="$(dirname "$(find "$root/target/dx/stowarzysz/release/android" -name gradlew -type f | head -n 1)")"
[ -f "$gradle_dir/gradlew" ] || { echo "dx produced no Gradle project" >&2; exit 1; }
sed -i.bak "s/versionCode = [0-9]*/versionCode = $code/" "$gradle_dir/app/build.gradle.kts"
rm -f "$gradle_dir/app/build.gradle.kts.bak"
grep -q "versionCode = $code" "$gradle_dir/app/build.gradle.kts"
(cd "$gradle_dir" && ./gradlew --quiet assembleRelease)

unsigned="$(find "$gradle_dir/app/build/outputs/apk/release" -name '*.apk' | head -n 1)"
mkdir -p "$root/dist"
out="$root/dist/stowarzysz-$version.apk"
"$build_tools/zipalign" -P 16 -f 4 "$unsigned" "$root/dist/aligned.apk"
"$build_tools/apksigner" sign \
    --ks "$ANDROID_KEYSTORE" --ks-key-alias "$key_alias" \
    --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEYSTORE_PASSWORD \
    --out "$out" "$root/dist/aligned.apk"
rm -f "$root/dist/aligned.apk" "$out.idsig"
"$build_tools/apksigner" verify "$out"
echo "$out"
