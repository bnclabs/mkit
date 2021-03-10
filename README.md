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

Useful links
------------

* [xorfilter][xorfilter-blog] by Daniel Lemire.
* [Wiki link][spinlock] on spinlock.
* [RFC specification][cbor-rfc] for CBOR.

Contribution
------------

* Simple workflow. Fork - Modify - Pull request.
* Before creating a PR,
  * Run `make build` to confirm all versions of build is passing with
    0 warnings and 0 errors.
  * Run `check.sh` with 0 warnings, 0 errors and all testcases passing.
  * Run `perf.sh` with 0 warnings, 0 errors and all testcases passing.
  * [Install][spellcheck] and run `cargo spellcheck` to remove common spelling mistakes.
* [Developer certificate of origin][dco] is preferred.

[xorfilter]: https://github.com/bnclabs/xorfilter
[xorfilter-blog]: https://lemire.me/blog/2019/12/19/xor-filters-faster-and-smaller-than-bloom-filters/
[spinlock]: https://en.wikipedia.org/wiki/Spinlock
[cbor-rfc]: https://tools.ietf.org/html/rfc7049
[spellcheck]: https://github.com/drahnr/cargo-spellcheck
[dco]: https://developercertificate.org/
