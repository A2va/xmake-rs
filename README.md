# xmake

A build dependency for running the `xmake` build tool to compile a native
library.

```toml
# Cargo.toml
[build-dependencies]
xmake = "0.1"
```

The XMake executable is assumed to be `xmake` unless the `XMAKE`
environmental variable is set.
