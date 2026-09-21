#!/bin/bash
# Source this to build raydriver with rayforge's pixi toolchain.
export PATH=/root/projects/rayforge/.pixi/envs/default/bin:$PATH
export CC=x86_64-conda-linux-gnu-cc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-conda-linux-gnu-cc
