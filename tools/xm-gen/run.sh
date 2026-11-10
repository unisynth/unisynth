#!/bin/sh
# Generator maps of the softsynth intros (change add-plain-xm-import): a venv with unicorn + capstone in out/venv,
# then extract.py [intro ...]. Material from $ZENEKK_DIR (default ~/Downloads/zenekk).
set -e
cd "$(dirname "$0")"
if [ ! -x out/venv/bin/python ]; then
    python3 -m venv out/venv
    out/venv/bin/pip install -q unicorn==2.1.4 capstone==5.0.7
fi
exec out/venv/bin/python extract.py "$@"
