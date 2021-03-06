0.4.0 (mkit-derive 0.3.0)
=========================

* implement NoBitmap type, for dummy `Bloom` implementations.
* cbor: fix bugs.
* cborize: fix bugs in procedural macros.
* db: changes to `Bloom` trait definition.
* db: implement `Entry::purge()`.
* db: add api to drain deltas from Entry.
* implement Bloom and Cborize traits for Xorfilter.
* package management files
* ci scripts.

0.3.0
=====

* rust-fmt fix column width to 90.
* db: add `insert()` and `delete()` API for diff mechanics.
* db: add `as_key()` method for Entry.
* db: implement `Diff` for basic types.
* db: implement Eq, PartialEq, Debug traits for db-types.
* implement spinlock for non-blocking rw-exclusion.
* clippy fixes.

0.2.0
=====

* bug fixes to `Cborize` procedural macro.
* Implement `LocalCborize` for types local to `mkit`.
* traits: new traits `Diff`, `Bloom`.
* thread: rename `Thread::close_wait()` method to `join()`.
* rustfmt: fix column-width to 90.
* db: implement Entry, Value and Delta types.
* cbor: implement `FromCbor` and `IntoCbor` for `OsString`.
* cbor: support break-stop encoding.
* cbor: implement `IntoCbor` for `cbor::SimpleValue`.


Refer to [release-checklist][release-checklist].

[release-checklist]: https://prataprc.github.io/rust-crates-release-checklist.html
