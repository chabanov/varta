# Varta rusqlite file-identity patch

This is the crates.io rusqlite 0.40.1 release, upstream rusqlite/rusqlite
commit `6d3c282dc5531a57eb4e22ece3207f00c95d0fb0`, under the retained upstream license files.
Only `src/lib.rs` changes; VARTA-PATCH.diff reproduces the patch. Registry
bookkeeping, original manifest, CI and ignore files are omitted. Version pins
and SQLite storage/schema/locking behavior remain unchanged.

Varta must verify the actual file behind each SQLite connection before reading
its schema or writing. Process fd snapshots cannot prove that identity: SQLite
legitimately reuses cached descriptors while another connection holds a lock;
macOS /dev/fd stat also reports a devfs device and mutable timestamps are not
identity. No process-wide serialization or retry can replace that proof.

The paired patches add a read-only private file-control operation (100001) to
the bundled Unix VFS and a safe Connection::main_file_identity wrapper. The
wrapper checks the selected VFS; the SQLite dispatcher additionally checks the
concrete xFileControl function pointer before forwarding the private opcode, so
a third-party VFS cannot collide with its output-buffer contract even by
registering the name unix. The wrapper borrows the live
connection, and supplies exactly two initialized/aligned u64 outputs. The VFS
uses fstat on its own concrete fd, including a cached/reused fd, and returns
actual device/inode. No path lookup, open, dup, close, locking or writes occur.
Unsupported VFSes, memory databases and unpatched SQLite fail closed. No unsafe
code is introduced into workspace crates; FFI stays in the existing vendor layer.

Runtime regressions cover actual identity, changed timestamps, cached-fd reuse,
swapped-in cached replacements, symbolic links, read/write locks and 400 parallel
HTTP requests with two opens each. Remove these patches when an equivalent safe
upstream concrete-file identity API is available. No guest API or Wasm ABI changes.
