#!/bin/sh

OS="$(uname)"

# Install dependencies (best effort)

if [ ${OS} = "Darwin" ] ; then
    export CFLAGS=-I/opt/homebrew/opt/gmp/include LDFLAGS=-L/opt/homebrew/opt/gmp/lib
fi 

set -e

. scripts/cairo-rs-py/bin/activate
maturin develop --release
cd kakarot
pip install poetry
make setup
make build
cd ..
cd scripts
patch --directory cairo-rs-py/lib/python3.9/site-packages/ --strip 2 < move-to-cairo-rs-py.patch
cd ..
deactivate
. scripts/cairo-lang/bin/activate
maturin develop --release
cd kakarot
pip install poetry
make setup
make build
deactivate
cd ..