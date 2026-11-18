#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

app="$HOME/Applications/Bronze.app"
staged=target/mac-app/Bronze.app
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)

cargo build --release

rm -rf target/mac-app
mkdir -p "$staged/Contents/MacOS" "$staged/Contents/Resources" target/mac-app/Bronze.iconset
cp target/release/bronze "$staged/Contents/MacOS/bronze"

for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon-1024.png --out "target/mac-app/Bronze.iconset/icon_${size}x${size}.png" > /dev/null
  sips -z $((size * 2)) $((size * 2)) assets/icon-1024.png --out "target/mac-app/Bronze.iconset/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns target/mac-app/Bronze.iconset -o "$staged/Contents/Resources/Bronze.icns"

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
else
  codesign --force --sign - "$staged"
  echo "mac-app: no signing identity, so Bronze is signed ad hoc and macOS will forget its Accessibility permission on every rebuild" >&2
fi

mkdir -p "$HOME/Applications"
rm -rf "$app"
ditto "$staged" "$app"
echo "mac-app: installed $app ($version)"
