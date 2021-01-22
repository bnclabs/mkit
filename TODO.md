* Implement a command line tool to serialize cbor value into bytes and deserialize them
  back and render their shape and content on terminal.
* Double check the requirement for `log` package. Should we log or just return errors ?
* Implement Diff for basic-types:
  array, slice, string, Vec, tuple.
* Implement NoDiff as procedural macro on any value struct or enum.
