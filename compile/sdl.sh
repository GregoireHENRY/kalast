#!/bin/bash

echo "matrix: $MATRIX_OS"
echo "matrix: ${{ matrix.os }}"

if [ $MATRIX_OS == "ubuntu-latest" ]; then
    sudo apt install -y libsdl2-dev
elif [ ${{ matrix.os }} == "macos-latest" ]; then
    brew install SDL2
elif [ ${{ matrix.os }} == "windows-latest" ]; then
    cp include/windows/* .
fi