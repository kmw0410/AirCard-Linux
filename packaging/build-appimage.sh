#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
appdir="$root/dist/AirCard.AppDir"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
rm -rf "$appdir"
cargo build --manifest-path "$root/Cargo.toml" --release
install -Dm755 "$target_dir/release/aircard" "$appdir/usr/bin/aircard"
for tool in idevice_id ideviceinfo idevicesyslog; do
  command -v "$tool" >/dev/null || { echo "Missing $tool (install libimobiledevice utilities)" >&2; exit 1; }
  install -Dm755 "$(command -v "$tool")" "$appdir/usr/bin/$tool"
done
install -Dm644 "$root/resources/io.github.aircard.AirCard.desktop" "$appdir/usr/share/applications/io.github.aircard.AirCard.desktop"
install -Dm644 "$root/resources/io.github.aircard.AirCard.metainfo.xml" "$appdir/usr/share/metainfo/io.github.aircard.AirCard.appdata.xml"
install -Dm644 "$root/LICENSE" "$appdir/usr/share/licenses/aircard/LICENSE"
install -Dm644 "$root/resources/io.github.aircard.AirCard.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.aircard.AirCard.svg"
cp "$root/resources/io.github.aircard.AirCard.desktop" "$appdir/io.github.aircard.AirCard.desktop"
cp "$root/resources/io.github.aircard.AirCard.svg" "$appdir/io.github.aircard.AirCard.svg"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export LD_LIBRARY_PATH="$HERE/usr/lib:${LD_LIBRARY_PATH:-}"
export PATH="$HERE/usr/bin:$PATH"
export XDG_DATA_DIRS="$HERE/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "$HERE/usr/bin/aircard" "$@"
EOF
chmod +x "$appdir/AppRun"
if ! command -v linuxdeploy >/dev/null; then
  echo "linuxdeploy is required to bundle GTK and libimobiledevice" >&2
  exit 1
fi
if ! command -v appimagetool >/dev/null; then
  echo "appimagetool is required (https://github.com/AppImage/AppImageKit/releases)" >&2
  exit 1
fi
linuxdeploy --appdir "$appdir" \
  --executable "$appdir/usr/bin/aircard" \
  --executable "$appdir/usr/bin/idevice_id" \
  --executable "$appdir/usr/bin/ideviceinfo" \
  --executable "$appdir/usr/bin/idevicesyslog"
# Validation may try to reach the metadata homepage; packaging must also work offline.
ARCH=x86_64 appimagetool --no-appstream "$appdir" "$root/dist/AirCard-x86_64.AppImage"
