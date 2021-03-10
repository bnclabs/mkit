[![Documentation](https://docs.rs/mkit/badge.svg?style=flat-square)](https://docs.rs/mkit)

Mkit is a collection of traits, utilities and common useful types required
to build distributed, peer-to-peer applications.

* __cbor__, Concise Binary Object Representation (CBOR) implementation.
* __thread__, a Thread type for multi-threading associated channel types
  for inter-process-communication.
* __spinlock__, for non-blocking read-write locking using atomic load/store/cas.
* __traits for data__, Diff.
* __types for db__, Entry, Value, Delta, Cutoff.
* __traits for db__, BuildIndex, Bloom.
* __xor-filter__, implement Bloom trait for [xorfilter][xorfilter] type.

Contribution
------------

* Simple workflow. Fork, modify and raise a pull request.
* Before making a PR,
  * [Install][spellcheck] and run `cargo spellcheck` to remove common spelling mistakes.
  * Run `check.sh` with 0 warnings, 0 errors and all testcases passing.
* [Developer certificate of origin][dco] is preferred.

[spellcheck]: https://github.com/drahnr/cargo-spellcheck
[dco]: https://developercertificate.org/
[xorfilter]: https://github.com/bnclabs/xorfilter
