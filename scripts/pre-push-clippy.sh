#!/usr/bin/env bash
# Only run strict clippy (dead_code included) when pushing to main.
set -e

while read -r _local_ref _local_sha remote_ref _remote_sha; do
    if [ "$remote_ref" = "refs/heads/main" ]; then
        cargo clippy -- -D warnings
        exit 0
    fi
done

exit 0
