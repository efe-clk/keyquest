#!/usr/bin/env bash
# Builds target/KeyQuest-<version>-x86_64.AppImage with linuxdeploy and its
# GTK plugin. GTK 4 and libadwaita come from the build machine and are bundled,
# so build on the oldest distribution you want to support: the AppImage runs on
# systems with the same or a newer glibc.
#
# Needs: cargo, libgtk-4-dev, libadwaita-1-dev, curl, file.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tools="${KEYQUEST_APPIMAGE_TOOLS:-$root/target/appimage-tools}"
appdir="$root/target/AppDir"
app_id="io.github.efe_clk.KeyQuest"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n1)"

linuxdeploy="$tools/linuxdeploy-x86_64.AppImage"
gtk_plugin="$tools/linuxdeploy-plugin-gtk.sh"
mkdir -p "$tools"
if [ ! -x "$linuxdeploy" ]; then
    curl -fsSL -o "$linuxdeploy" \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
    chmod +x "$linuxdeploy"
fi
if [ ! -x "$gtk_plugin" ]; then
    curl -fsSL -o "$gtk_plugin" \
        https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh
    chmod +x "$gtk_plugin"
fi

cargo build --release --locked -p keyquest-gtk --manifest-path "$root/Cargo.toml"

rm -rf "$appdir"
install -Dm644 "$root/data/$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.appdata.xml"

# Containers and CI runners usually have no FUSE.
export APPIMAGE_EXTRACT_AND_RUN=1
export DEPLOY_GTK_VERSION=4
# Metainfo is validated offline in CI; the online check fails while the repo is private.
export LDAI_NO_APPSTREAM=1
export LINUXDEPLOY_OUTPUT_VERSION="$version"
export PATH="$tools:$PATH"

cd "$root/target"
"$linuxdeploy" \
    --appdir "$appdir" \
    --executable "$root/target/release/keyquest" \
    --desktop-file "$root/data/$app_id.desktop" \
    --icon-file "$root/data/icons/$app_id.svg" \
    --plugin gtk

# The GTK plugin forces GTK_THEME=Adwaita, which replaces libadwaita's own
# stylesheet. libadwaita follows the system light/dark preference by itself.
hook="$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh"
grep -q '^export GTK_THEME=' "$hook"
sed -i '/^export GTK_THEME=/d' "$hook"

# Symbolic icons from the Adwaita theme, for desktops that do not have it.
install -Dm644 -t "$appdir/usr/share/icons/hicolor/scalable/actions" "$root"/data/icons/symbolic/*.svg

"$linuxdeploy" --appdir "$appdir" --output appimage

ls -l "$root/target/"KeyQuest-*.AppImage
