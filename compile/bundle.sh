#!/bin/bash

mkdir -p bundle
cp -r examples bundle
cp target/release/kalast$EXT bundle
cp target/release/examples/viewer-picker$EXT bundle/examples/viewer-picker
cp include/kalast.ico bundle
cp -r include/assets bundle
cp preferences.yaml bundle
cp README.md bundle

if [ ${{ matrix.os }} == "windows-latest" ]; then
    cp include/windows/* bundle
fi

cd bundle
cp -r examples/viewer/cfg .

cd ..
mv bundle $BUNDLE_NAME

if [ ${{ matrix.os }} == "windows-latest" ]; then
    iscc compile/installer.iss /DVERSION=${{ github.ref_name }} /DSETUP_NAME=${{ env.RELEASE_FILE }} /DBUNDLE_PATH=.\bundle /DASSETS_PATH=.\assets
else
    tar cvzf $RELEASE_FILE $BUNDLE_NAME
fi