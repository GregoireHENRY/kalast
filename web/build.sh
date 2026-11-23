#!/bin/sh
# kalast for the browser, built into web/dist/, which is the whole site:
#
# - the stable page, which kalast.greghy.com opens: the newest release's,
#   built from its tag the first time and kept in target/pages/;
# - the beta, at beta/: this tree's, shown instead to a visitor who ticked
#   "offer betas" in the app tab (index.html).
#
# A page is its index.html, and its script and WebAssembly in a folder named
# for their content, so that a browser keeping files a few hours never mixes
# an old one with a new page. `./build.sh --page DIR` builds this tree's page
# alone, into DIR. Needs the wasm32 target and wasm-bindgen at the version
# web/Cargo.toml pins:
#
#     rustup target add wasm32-unknown-unknown
#     cargo install wasm-bindgen-cli --version 0.2.128
#
# The site as it will go up (`npx wrangler deploy` from web/), on this
# machine:
#
#     python3 -m http.server 8777 --bind 127.0.0.1 --directory web/dist
#
# then http://127.0.0.1:8777/beta/ for this tree's page.
set -e
cd "$(dirname "$0")"
web=$PWD

# This tree's page, into $1.
page() {
    if [ -e "$1" ] && [ ! -f "$1/index.html" ] && [ ! -d "$1/pkg" ]; then
        echo "$1 is not a page's folder: left as it is" >&2
        exit 1
    fi
    cargo build --release
    rm -rf "$1"
    mkdir -p "$1"
    out=$(cd "$1" && pwd)
    wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" \
        "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/kalast_web.wasm"
    # The page's Python (python.js): kalast's Python package as the repository
    # has it, and in place of its extension module the page's own, python/.
    # What a page showing this one sends: embed.js. A mouse wheel in lines:
    # wheel.js.
    cp python.js folder.js embed.js wheel.js "$out/pkg/"
    # The tab's icon: the logo, the desktop window's icon too.
    cp ../res/kalast-256.png "$out/pkg/"
    (cd .. && zip -qrX "$out/pkg/kalast-python.zip" kalast -i '*.py' '*.mplstyle' -x '*/__pycache__/*' 'kalast/_rs*')
    (cd python && zip -qrX "$out/pkg/kalast-python.zip" . -i '*.py' -x '*/__pycache__/*')
    version=$(cat "$out"/pkg/* | shasum | cut -c1-12)
    mv "$out/pkg" "$out/$version"
    sed -e "s/__VERSION__/$version/g" -e "s/__BUILT__/$(date -u +%Y-%m-%d)/" index.html > "$out/index.html"
}

if [ "$1" = --page ]; then
    page "$2"
    echo "$2 ready: $version"
    exit
fi

# The stable page: the newest release that has one. None has yet -- 0.5.15
# will be the first -- and until one has, the page of the commit named below
# (23 November: the page of 9 November, kept until then, no longer took clicks).
stable=$(git tag --list 'v[0-9]*' --sort=-v:refname | grep -v -e - | while read -r tag; do
    if git cat-file -e "$tag:web/build.sh" 2>/dev/null; then echo "$tag"; break; fi
done)
stable=${stable:-5b164ed9d63a}
kept=target/pages/$stable
if [ ! -f "$kept/index.html" ]; then
    src=$(mktemp -d)
    trap 'git worktree remove --force "$src/kalast" 2>/dev/null; rmdir "$src"' EXIT
    git worktree add --detach "$src/kalast" "$stable"
    # This tree's Cargo.lock, which git does not keep: the dependencies the
    # beta is built with, rather than whatever is newest that day.
    [ ! -f Cargo.lock ] || cp Cargo.lock "$src/kalast/web/"
    CARGO_TARGET_DIR="$web/target/stable" "$src/kalast/web/build.sh" --page "$web/$kept"
fi

page target/pages/beta
rm -rf dist
mkdir dist
cp -R "$kept"/. dist/
cp -R target/pages/beta dist/beta
channel() {
    sed "s/__CHANNEL__/$2/" "$1/index.html" > "$1/index.tmp"
    mv "$1/index.tmp" "$1/index.html"
}
channel dist stable
channel dist/beta beta
folder() { basename "$(find "$1" -mindepth 1 -maxdepth 1 -type d ! -name beta)"; }
sed -e "s/__STABLE__/$(folder dist)/" -e "s/__BETA__/$(folder dist/beta)/" _headers > dist/_headers
echo "dist/ ready, $(du -sh dist | cut -f1): the stable page $stable's ($(folder dist)), the beta this tree's ($(folder dist/beta))"
