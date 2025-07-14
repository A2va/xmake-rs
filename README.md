# xmake

A build dependency for running the [xmake](https://xmake.io/) build tool to compile a native
library.

```toml
# Cargo.toml
[build-dependencies]
xmake = "0.3.3"
```

The XMake executable is assumed to be `xmake` unless the `XMAKE`
environmental variable is set.
There is some examples in the `tests` folder of the repo. 

## Difference from cmake-rs

Broadly speaking, xmake-rs is very similar to cmake-rs, but there are two advantages:
* Xmake is known to be simpler than cmake, with a built-in package manager so using it improve the development workflow.
* Xmake-rs supports automatic linking, so it is no longer necessary to use `rustc-link-lib` and `rustc-link-search` as in cmake-rs.

## Cross-Compilation Support

If you need to cross-compile your project, xmake provides a built-in package manager that can set up the emscripten or Android NDK toolchains. The first two lines of the code snippet below enter a single package environment, overwriting the previous environment. However, the last line enters both the emscripten and NDK environments simultaneously.
```
xrepo env -b ndk shell
xrepo env -b emscripten shell
xrepo env -b "emscripten, ndk" shell
```
After executing one of these commands, xmake will automatically detect either the emscripten or NDK toolchain.

If you prefer to use your own toolchain, you can set either the ANDROID_NDK_HOME or EMSCRIPTEN_HOME environment variables to specify the path to the corresponding toolchain.
