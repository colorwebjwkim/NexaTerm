# NexaTerm local patch to russh-sftp 3.0.1

Upstream: https://github.com/AspectUnk/russh-sftp, crates.io 3.0.1, Apache-2.0 (LICENSE retained).

Modified 2026-10-14: annotate protocol handle fields (including fsync and borrowed WRITE) with a lossless opaque-byte serde codec. Upstream decodes `String` with `from_utf8_lossy`, corrupting arbitrary binary server handles, reproduced against an independent ssh2 server at handle 128. UTF-8 handles keep their existing internal representation; binary handles and the reserved prefix use a reversible hex surrogate internally. Only handle fields use this codec; wire bytes are preserved exactly. No filenames or other protocol strings are re-encoded.

The application regression test uses binary handles from 0 through 255 and a literal reserved-prefix handle. The application E2E fixture intentionally continues returning four-byte binary handles and transfers 300 files in both directions. Non-UTF8 filenames remain unsupported and are conservatively blocked by the application. Remove/rebase this patch only after upstream provides and verifies a binary-handle API.

Additional changes: cap high-level read_dir at 10,000 entries and reject empty progress pages; close directory handles on error. Rename deprecated AtomicU64::fetch_update to try_update for the project's Rust 1.99 toolchain. These are maintained local changes, not claims about an upstream release.
