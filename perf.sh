#! /usr/bin/env bash

exec > perf.out
exec 2>&1

set -o xtrace

date; time cargo +nightly bench -- --nocapture || exit $?
