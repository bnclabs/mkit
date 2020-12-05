[![Rustdoc](https://img.shields.io/badge/rustdoc-hosted-blue.svg)](https://docs.rs/mkit)

Mkit is a collection of traits, utilities and common useful types required
to build distributed, peer-to-peer applications.

* __cbor__, Concise Binary Object Representation (CBOR) implementation.
* __thread__, a Thread type for multi-threading associated channel types
  for inter-process-communication.

Contribution
------------

* Simple workflow. Fork, modify and raise a pull request.
* Before making a PR,
  * Run `cargo build` to make sure 0 warnings and 0 errors.
  * Run `cargo test` to make sure all test cases are passing.
  * Run `cargo bench` to make sure all benchmark cases are passing.
  * Run `cargo +nightly clippy --all-targets --all-features` to fix clippy issues.
  * [Install] and run `cargo spellcheck` to remove common spelling mistakes.
* [Developer certificate of origin][dco] is preferred.

[spellcheck]: https://github.com/drahnr/cargo-spellcheck
