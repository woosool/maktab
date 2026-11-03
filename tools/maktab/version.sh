#!/bin/sh
# Maktab's version: Qalam's (all the apps share it; see the README).
grep -m1 '^version' "$(dirname "$0")/../../qalam/app/Cargo.toml" | cut -d'"' -f2
