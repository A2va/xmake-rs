The test naming in this folder is a folllows:
- `stt`: static targert
- `sht`: shared target
- `stf`: static foo (package name)
- `shf`: shared foo (package name)
- `stb`: static bar
- `shb`: shared bar

So a test name like: `stt-shf-stb`, test in the following configuration: static target, shared foo, static bar.

For these tests to work correctly policy `package.librarydeps.strict_compatibility` had to be enabled,  because otherwise the bar shared libraries in the example will referenced foo even in the second snippet because the package hash of bar would be the same.
```lua
add_requires("xmrs-bar", {configs = {shared = true}})
add_requireconfs("xmrs-bar.xmrs-foo", {configs = {shared = true}})
```
And another that only requires bar. 
```lua
add_requires("xmrs-bar", {configs = {shared = true}})
```