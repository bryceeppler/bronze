#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

case $#:${1:-} in
  0: | 1:--package) mode=${1:-} ;;
  *) echo "usage: scripts/mac-app.sh [--package]" >&2; exit 2 ;;
esac

app="$HOME/Applications/Bronze.app"
staged=target/mac-app/Bronze.app
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)

rm -rf target/mac-app
mkdir -p "$staged/Contents/MacOS" "$staged/Contents/Resources"

if [ "$mode" = --package ]; then
  targets=(aarch64-apple-darwin x86_64-apple-darwin)
  rustup target add "${targets[@]}"
  CARGO_ENCODED_RUSTFLAGS=$(printf '%s\x1f%s' "--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo" "--remap-path-prefix=$PWD=/bronze")
  export CARGO_ENCODED_RUSTFLAGS
  slices=()
  for target in "${targets[@]}"; do
    cargo build --release --locked --target "$target" --target-dir target/package-build
    slices+=("target/package-build/$target/release/bronze")
  done
  lipo -create -output "$staged/Contents/MacOS/bronze" "${slices[@]}"
else
  cargo build --release
  cp target/release/bronze "$staged/Contents/MacOS/bronze"
fi
cp assets/brand/icon/bronze-dark.icns "$staged/Contents/Resources/Bronze.icns"

cat > "$staged/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>ca.bryce.bronze</string>
  <key>CFBundleName</key><string>Bronze</string>
  <key>CFBundleDisplayName</key><string>Bronze</string>
  <key>CFBundleExecutable</key><string>bronze</string>
  <key>CFBundleIconFile</key><string>Bronze</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

identities=$(security find-identity -v -p codesigning | awk '/"Apple Development/ {print $2}')
identity=${BRONZE_SIGN:-}
if [ -z "$identity" ] && [ "$(printf '%s' "$identities" | grep -c .)" -gt 1 ]; then
  echo "mac-app: several Apple Development identities; set BRONZE_SIGN to the one to use" >&2
  exit 1
fi
identity=${identity:-$identities}

if [ -n "$identity" ]; then
  if ! signed=$(codesign --force --sign "$identity" "$staged" 2>&1); then
    echo "mac-app: signing failed: ${signed//"$identity"/<identity>}" >&2
    exit 1
  fi
elif [ "$mode" = --package ]; then
  echo "mac-app: a release needs an Apple Development identity to sign with" >&2
  exit 1
else
  codesign --force --sign - "$staged"
  echo "mac-app: no signing identity, so Bronze is signed ad hoc and macOS will forget its Accessibility permission on every rebuild" >&2
fi

if [ "$mode" = --package ]; then
  mkdir -p target/package
  zip=target/package/Bronze-$version-macos-universal.zip
  rm -f "$zip"
  ditto -c -k --norsrc --noextattr --keepParent "$staged" "$zip"
  echo "mac-app: packaged $zip"
  exit 0
fi

mkdir -p "$HOME/Applications"
rm -rf "$app"
ditto "$staged" "$app"
echo "mac-app: installed $app ($version)"
