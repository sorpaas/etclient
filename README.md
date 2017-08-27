# Etclient

[![Build Status](https://travis-ci.org/sorpaas/etclient.svg?branch=master)](https://travis-ci.org/sorpaas/etclien)

An bare minimum Ethereum client built on top of SputnikVM and
`etcommon`. See [this page](https://that.world/~source/etclient.html)
for the current progress.

You can try this out by installing Rust and then run:

```
cargo run --release
```

Currently we have a full block validator working, and it is able to
sync the blockchain with the network. Note that the storage is
currently in-memory.
