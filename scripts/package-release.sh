#!/usr/bin/env bash
set -euo pipefail

binary=${1:?expected path to the release binary}
target=${2:?expected Rust target triple}
format=${3:?expected archive format}
out_dir=${4:?expected output directory}
tag=${5:-}

version=$("$binary" version)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+$ ]]; then
    printf 'unexpected version: %s\n' "$version" >&2
    exit 1
fi
if [[ -n "$tag" && "$tag" != "v$version" ]]; then
    printf 'release tag %s does not match binary version %s\n' "$tag" "$version" >&2
    exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
mkdir -p -- "$stage/package" "$stage/verify"
name=${binary##*/}
cp -- "$binary" "$stage/package/$name"
archive="$stage/knowbox-v$version-$target.$format"

case "$format" in
    tar.gz)
        tar -czf "$archive" -C "$stage/package" "$name"
        tar -xzf "$archive" -C "$stage/verify"
        ;;
    zip)
        # Use Windows' libarchive tar instead of the GNU tar on Git Bash's PATH.
        windows_tar="$(cygpath -u "${SYSTEMROOT:-${SystemRoot:?expected Windows system root}}")/System32/tar.exe"
        "$windows_tar" -a -cf "$archive" -C "$stage/package" "$name"
        "$windows_tar" -xf "$archive" -C "$stage/verify"
        ;;
    *)
        printf 'unsupported archive format: %s\n' "$format" >&2
        exit 1
        ;;
esac

packaged="$stage/verify/$name"
[[ $("$packaged" version) == "$version" ]]
[[ $("$packaged" --version) == "knowbox $version" ]]
unset KNOWBOX_ROOT KNOWBOX_EMBED_URL
kb_root="$stage/kb"
if [[ "$format" == zip ]]; then
    kb_root=$(cygpath -w "$kb_root")
fi
"$packaged" init --no-git "$kb_root"
export KNOWBOX_ROOT="$kb_root"
"$packaged" new --type fact --domain release --title 'Release smoke test' --body 'uniquereleasesmoketoken' --no-edit
"$packaged" search uniquereleasesmoketoken > "$stage/search.txt"
grep -q '^kb-0001 ' "$stage/search.txt"

mkdir -p -- "$out_dir"
cp -- "$archive" "$out_dir/"
