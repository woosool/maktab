#!/bin/sh
# The app version from app/Cargo.toml.
grep -m1 '^version' "$(dirname "$0")/../app/Cargo.toml" | cut -d'"' -f2
