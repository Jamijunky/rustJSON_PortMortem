# Reference sources — provenance and checksums

These are pristine copies of the upstream cJSON v1.7.19 reference sources
used by `build.rs` and the differential tests. They are byte-identical to
the upstream commit and are never modified by this project.

- Upstream: https://github.com/DaveGamble/cJSON
- Commit:   `fb16e5cf358798aabb049655975cde8427101056` (v1.7.19)
- License:  MIT

Verify with: `shasum -a 256 -c HASHES.txt` (or run `scripts/verify_vendored.sh`).
