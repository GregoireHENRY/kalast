#!/bin/bash

if [ ${{ matrix.os }} == "ubuntu-latest" ]; then
    echo "PLATFORM=ubuntu" >> "$GITHUB_ENV"
elif [ ${{ matrix.os }} == "macos-latest" ]; then
    echo "PLATFORM=macos-x86_64" >> "$GITHUB_ENV"
elif [ ${{ matrix.os }} == "windows-latest" ]; then
    echo "PLATFORM=windows" >> "$GITHUB_ENV"
fi

if [ ${{ matrix.os }} == "windows-latest" ]; then
    echo "EXT=.exe" >> "$GITHUB_ENV"
    echo "OUT_EXT=.exe" >> "$GITHUB_ENV"
else
    echo "EXT=" >> "$GITHUB_ENV"
    echo "OUT_EXT=.tar.gz" >> "$GITHUB_ENV"
fi

echo "BUNDLE_NAME=kalast-$GITHUB_REF_NAME-$PLATFORM" >> "$GITHUB_ENV"
echo "RELEASE_FILE=$BUNDLE_NAME$OUT_EXT" >> "$GITHUB_ENV"

echo $PLATFORM
echo $EXT
echo $OUT_EXT
echo $BUNDLE_NAME
echo $RELEASE_FILE