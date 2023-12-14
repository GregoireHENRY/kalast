#!/bin/bash

if [ ${{ matrix.os }} == "ubuntu-latest" ]; then
    sudo apt install -y libsdl2-dev
elif [ ${{ matrix.os }} == "macos-latest" ]; then
    brew install SDL2
elif [ ${{ matrix.os }} == "windows-latest" ]; then
    cp include/windows/* .
fi