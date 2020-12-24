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

Release Checklist
=================

* Documentation Review.
* Bump up the version:
  * __major__: backward incompatible API changes.
  * __minor__: backward compatible API Changes.
  * __patch__: bug fixes.
* README
  * Link to rust-doc.
  * Short description.
  * Contribution guidelines.
* Cargo checklist
  * check.sh
* Cargo spell check.
* Create a git-tag for the new version.
* Cargo publish the new version.

(optional)

* Travis-CI integration.
* Badges
  * Build passing, Travis continuous integration.
  * Code coverage, codecov and coveralls.
  * Crates badge
  * Downloads badge
  * License badge
  * Rust version badge.
  * Maintenance-related badges based on isitmaintained.com
  * Documentation
  * Gitpitch

