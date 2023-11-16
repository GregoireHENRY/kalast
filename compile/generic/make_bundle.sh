#!/bin/bash

# $GITHUB_REF_NAME is an env defined by GitHub automatically representing the
# short ref name of the branch or tag that triggered the workflow.
# For local run, it should be set by the user to a versionning number like
# `v0.3.7`.

# $PLATFORM should be defined in env.
# Can be either:
# - ubuntu
# - macos-x86_64
# - macos-arm64

echo "Create bundle folder."
mkdir -p bundle

echo "Copy examples and executables to bundle."
cp -r examples bundle
cp target/release/kalast bundle
cp target/release/examples/viewer-picker bundle/examples/viewer-picker
cp include/kalast.ico bundle
cp include/preferences.yaml bundle
cp README.md bundle/content

echo "Move inside bundle."
cd bundle

echo "Select default cfg."
cp -r examples/thermal/cfg .

echo "Compress bundle."
tar cvzf kalast-$GITHUB_REF_NAME-$PLATFORM.tar.gz *