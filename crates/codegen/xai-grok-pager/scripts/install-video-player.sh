#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <release-package-directory>" >&2
    exit 2
}

[ "$#" -eq 1 ] || usage
source_dir=$1
[ -d "$source_dir" ] || { echo "Source directory does not exist: $source_dir" >&2; exit 1; }
source_dir=$(cd "$source_dir" && pwd -P)

case "$(uname -s)" in
    Darwin)
        os_kind=macos
        data_root=${XDG_DATA_HOME:-"$HOME/Library/Application Support"}
        destination="$data_root/DX/Video"
        library_glob='*.dylib'
        ;;
    Linux)
        os_kind=linux
        data_root=${XDG_DATA_HOME:-"$HOME/.local/share"}
        destination="$data_root/dx/video"
        library_glob='*.so*'
        ;;
    *)
        echo "The DX video-player installer supports macOS and Linux only." >&2
        exit 1
        ;;
esac

source_exe="$source_dir/dx-video-player"
[ -f "$source_exe" ] || { echo "Missing player executable: $source_exe" >&2; exit 1; }
[ -x "$source_exe" ] || { echo "Player is not executable: $source_exe" >&2; exit 1; }

parent=$(dirname "$destination")
mkdir -p "$parent"
staging="$parent/Video.staging-$$"
backup="$parent/Video.backup-$$"

cleanup() {
    [ ! -e "$staging" ] || rm -rf -- "$staging"
}
trap cleanup EXIT HUP INT TERM
mkdir "$staging"
cp "$source_exe" "$staging/dx-video-player"
chmod 755 "$staging/dx-video-player"

manifest_source="$source_dir/runtime-manifest.txt"
: > "$staging/runtime-manifest.txt"
if [ -f "$manifest_source" ]; then
    while IFS= read -r name || [ -n "$name" ]; do
        case "$name" in ''|'#'*) continue ;; esac
        case "$name" in */*|*'..'*) echo "Unsafe runtime manifest entry: $name" >&2; exit 1 ;; esac
        [ -f "$source_dir/$name" ] || { echo "Missing runtime file: $source_dir/$name" >&2; exit 1; }
        cp "$source_dir/$name" "$staging/$name"
        printf '%s\n' "$name" >> "$staging/runtime-manifest.txt"
    done < "$manifest_source"
else
    for library in "$source_dir"/$library_glob; do
        [ -f "$library" ] || continue
        name=$(basename "$library")
        cp "$library" "$staging/$name"
        printf '%s\n' "$name" >> "$staging/runtime-manifest.txt"
    done
fi

case "$os_kind" in
    macos)
        DYLD_LIBRARY_PATH="$staging${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
            "$staging/dx-video-player" --version >/dev/null 2>&1 || {
                echo "Staged macOS player cannot load its runtime libraries." >&2
                exit 1
            }
        ;;
    linux)
        LD_LIBRARY_PATH="$staging${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
            "$staging/dx-video-player" --version >/dev/null 2>&1 || {
                echo "Staged Linux player cannot load its runtime libraries." >&2
                exit 1
            }
        ;;
esac

if [ -e "$destination" ]; then
    rm -rf -- "$backup"
    mv "$destination" "$backup"
fi
if mv "$staging" "$destination"; then
    rm -rf -- "$backup"
else
    [ ! -e "$backup" ] || mv "$backup" "$destination"
    exit 1
fi

trap - EXIT HUP INT TERM
echo "DX video player installed to $destination"
