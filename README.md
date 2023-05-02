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

If you want to cross-compile, you must indicate the path of the NDK or emscripten via the environment variables `ANDROID_NDK_HOME` and `ANDROID_NDK_HOME`.

An example is available in the test-crate folder of the repo.