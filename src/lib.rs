//! A build dependency for running `xmake` to build a native library
//!
//! This crate provides some necessary boilerplate and shim support for running
//! the system `xmake` command to build a native library.
//!
//! The builder-style configuration allows for various variables and such to be
//! passed down into the build as well.
//!
//! ## Installation
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [build-dependencies]
//! xmake = "0.2.2"
//! ```
//!
//! ## Examples
//!
//! ```no_run
//! use xmake;
//!
//! // Builds the project in the directory located in `libfoo`, installing it
//! // into $OUT_DIR
//! let dst = xmake::build("libfoo");
//!
//! println!("cargo:rustc-link-search=native={}", dst.display());
//! println!("cargo:rustc-link-lib=static=foo");
//! ```
//!
//! ```no_run
//! use xmake::Config;
//!
//! let dst = Config::new("libfoo")
//!                 .option("bar", "true")
//!                 .env("XMAKE", "path/to/xmake")
//!                 .build();
//! println!("cargo:rustc-link-search=native={}", dst.display());
//! println!("cargo:rustc-link-lib=static=foo");
//! ```
#![deny(missing_docs)]

use std::collections::HashMap;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

/// Represents the different kinds of linkage for a library.
///
/// The `LinkKind` enum represents the different ways a library can be linked:
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// The library is statically linked, meaning its code is included directly in the final binary.
    Static,
    /// The library is dynamically linked, meaning the final binary references the library at runtime.
    Dynamic,
    /// The library is a system library, meaning it is provided by the operating system and not included in the final binary.
    System,
}

/// Represents a single linked library.
///
/// The `Link` struct contains information about a single linked library, including its name and the kind of linkage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The name of the linked library.
    name: String,
    /// The kind of linkage for the library.
    kind: LinkKind,
}

/// Represents the link information for a build.
///
/// The `BuildLinkInfo` struct contains information about the libraries that are linked in a build, including the directories they are located in and the individual `Link` structs.
#[derive(Default)]
pub struct BuildInfo {
    /// The directories that contain the linked libraries.
    directories: Vec<String>,
    /// The individual linked libraries.
    links: Vec<Link>,
    /// Whether the build uses the C++.
    use_cxx: bool,
    /// Whether the build uses the C++ standard library.
    use_stl: bool,
}

impl Link {
    /// Returns the name of the library as a string.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the kind of linkage for the library.
    pub fn kind(&self) -> &LinkKind {
        &self.kind
    }

    /// Creates a new `Link` with the given name and kind.
    pub fn new(name: &str, kind: LinkKind) -> Link {
        Link {
            name: name.to_string(),
            kind: kind,
        }
    }
}

impl BuildInfo {
    /// Returns the directories that contain the linked libraries.
    pub fn directories(&self) -> &[String] {
        &self.directories
    }

    /// Returns the individual linked libraries.
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// Returns whether the build uses C++.
    pub fn use_cxx(&self) -> bool {
        self.use_cxx
    }

    /// Returns whether the build uses C++ standard library.
    pub fn use_stl(&self) -> bool {
        self.use_stl
    }
}

/// Represents an error that occurred when parsing a `LinkKind` value from a string.
///
/// This error is returned when the string provided does not match any of the valid `LinkKind` variants.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseLindKindError;

impl FromStr for LinkKind {
    type Err = ParseLindKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "static" => Ok(LinkKind::Static),
            "shared" => Ok(LinkKind::Dynamic),
            "system" => Ok(LinkKind::System),
            _ => Err(ParseLindKindError),
        }
    }
}

/// Represents an error that occurred when parsing a `Link` struct from a string.
///
/// This error is returned when the string provided does not match the expected format for a `Link` struct.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseLinkError;

impl FromStr for Link {
    type Err = ParseLinkError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const NUMBER_OF_PARTS: usize = 2;

        let parts: Vec<_> = s.split("/").collect();
        if parts.len() != NUMBER_OF_PARTS {
            return Err(ParseLinkError);
        }

        let kind_result: LinkKind = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => return Err(ParseLinkError),
        };

        Ok(Link {
            name: parts[0].to_string(),
            kind: kind_result,
        })
    }
}

/// Represents an error that occurred when parsing `BuildLinkInfo` from a string.
///
/// This error is returned when the string provided does not match the expected format for `LinkInformation`.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseBuildInfoError;

impl FromStr for BuildInfo {
    type Err = ParseBuildInfoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const LIBRARY_FIELD: &str = "links";
        const DIRECTORY_FIELD: &str = "linkdirs";
        const CXX_FIELD: &str = "cxx_used";
        const STL_FIELD: &str = "stl_used";

        let mut map = parse_info_pairs(s);

        let keys = vec![LIBRARY_FIELD, DIRECTORY_FIELD, CXX_FIELD, STL_FIELD];
        for key in keys {
            if !map.contains_key(key) {
                return Err(ParseBuildInfoError);
            }
        }

        let directories: Vec<String> = match map.remove(DIRECTORY_FIELD) {
            Some(v) => v,
            None => return Err(ParseBuildInfoError),
        }; // directories are already strings

        let use_cxx = parse_field::<bool>(&map, CXX_FIELD)?;
        let use_stl = parse_field::<bool>(&map, STL_FIELD)?;

        let links: Vec<Link> = map[LIBRARY_FIELD]
            .iter()
            .map(|s| s.parse().map_err(|_| ParseBuildInfoError))
            .collect::<Result<_, _>>()?;

        Ok(BuildInfo {
            directories: directories,
            links: links,
            use_cxx: use_cxx,
            use_stl: use_stl,
        })
    }
}

#[derive(Default)]
struct ConfigCache {
    build_info: BuildInfo,
    plat: Option<String>,
    arch: Option<String>,
    env: HashMap<String, Option<String>>,
}

/// Builder style configuration for a pending XMake build.
pub struct Config {
    path: PathBuf,
    target: Option<String>,
    verbose: bool,
    auto_link: bool,
    out_dir: Option<PathBuf>,
    mode: Option<String>,
    options: Vec<(String, String)>,
    env: Vec<(String, String)>,
    static_crt: Option<bool>,
    cpp_link_stdlib: Option<String>,
    cache: ConfigCache,
}

/// Builds the native library rooted at `path` with the default xmake options.
/// This will return the directory in which the library was installed.
///
/// # Examples
///
/// ```no_run
/// use xmake;
///
/// // Builds the project in the directory located in `libfoo`, installing it
/// // into $OUT_DIR
/// let dst = xmake::build("libfoo");
///
/// println!("cargo:rustc-link-search=native={}", dst.display());
/// println!("cargo:rustc-link-lib=static=foo");
/// ```
///
pub fn build<P: AsRef<Path>>(path: P) -> PathBuf {
    Config::new(path.as_ref()).build()
}

impl Config {
    /// Creates a new blank set of configuration to build the project specified
    /// at the path `path`.
    pub fn new<P: AsRef<Path>>(path: P) -> Config {
        Config {
            path: env::current_dir().unwrap().join(path),
            target: None,
            verbose: false,
            auto_link: true,
            out_dir: None,
            mode: None,
            options: Vec::new(),
            env: Vec::new(),
            static_crt: None,
            cpp_link_stdlib: None,
            cache: ConfigCache::default(),
        }
    }

    /// Sets the xmake target for this compilation.
    /// Note that is different from rust target (os and arch), an xmake target
    /// can be binary or a library.
    pub fn target(&mut self, target: &str) -> &mut Config {
        self.target = Some(target.to_string());
        self
    }

    /// Sets verbose output.
    pub fn verbose(&mut self, value: bool) -> &mut Config {
        self.verbose = value;
        self
    }

    /// Configures if targets and their dependencies should be linked.
    ///
    /// This option defaults to `true`.
    pub fn auto_link(&mut self, value: bool) -> &mut Config {
        self.auto_link = value;
        self
    }

    /// Sets the output directory for this compilation.
    ///
    /// This is automatically scraped from `$OUT_DIR` which is set for Cargo
    /// build scripts so it's not necessary to call this from a build script.
    pub fn out_dir<P: AsRef<Path>>(&mut self, out: P) -> &mut Config {
        self.out_dir = Some(out.as_ref().to_path_buf());
        self
    }

    /// Sets the xmake mode for this compilation.
    pub fn mode(&mut self, mode: &str) -> &mut Config {
        self.mode = Some(mode.to_string());
        self
    }

    /// Configure an option for the `xmake` processes spawned by
    /// this crate in the `build` step.
    pub fn option<K, V>(&mut self, key: K, value: V) -> &mut Config
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.options
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    /// Configure an environment variable for the `xmake` processes spawned by
    /// this crate in the `build` step.
    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Config
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.env
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    /// Configures runtime type (static or not)
    ///
    /// This option defaults to `false`.
    pub fn static_crt(&mut self, static_crt: bool) -> &mut Config {
        self.static_crt = Some(static_crt);
        self
    }

    /// Set the standard library to link against when compiling with C++
    /// support (only Android).
    /// The given library name must not contain the `lib` prefix.
    ///
    ///
    /// Common values:
    /// - `c++_static`
    /// - `c++_shared`
    /// - `gnustl_static`
    /// - `gnustl_shared`
    /// - `stlport_shared`
    /// - `stlport_static`
    pub fn cpp_link_stdlib(&mut self, stblib: &str) -> &mut Config {
        self.cpp_link_stdlib = Some(stblib.to_string());
        self
    }

    /// Run this configuration, compiling the library with all the configured
    /// options.
    ///
    /// This will run both the configuration command as well as the
    /// command to build the library.
    pub fn build(&mut self) -> PathBuf {
        self.config();

        let mut cmd = self.xmake_command();
        cmd.arg("build");

        // In case of xmake is waiting to download something
        cmd.arg("--yes");
        if self.verbose {
            cmd.arg("-v");
        }

        if self.target.is_some() {
            cmd.arg(self.target.clone().unwrap());
        }

        run(&mut cmd, "xmake");

        // XMake put libary in the lib folder
        let dst = self.install().join("lib");
        println!("cargo:root={}", dst.display());

        if let Some(info) = self.get_build_info() {
            self.cache.build_info = info;
        }

        if self.auto_link {
            let build_info = &self.cache.build_info;

            for directory in build_info.directories() {
                // TODO: The optional KIND can be one of dependency, crate, native, framework, or all.
                // For now, framework is not supported, but eventually the kind must be set to all,
                // because the lua script cannot tag which directories belong to which kind.
                println!("cargo:rustc-link-search=native={}", directory);
            }

            for link in build_info.links() {
                match link.kind() {
                    LinkKind::Static => println!("cargo:rustc-link-lib=static={}", link.name()),
                    LinkKind::Dynamic => println!("cargo:rustc-link-lib=dylib={}", link.name()),
                    LinkKind::System => println!("cargo:rustc-link-lib={}", link.name()),
                }
            }
        }

        dst
    }

    // Run the configuration with all the configured
    /// options.
    fn config(&mut self) {
        let mut cmd = self.xmake_command();
        cmd.arg("config");

        // In case of xmake is waiting to download something
        cmd.arg("--yes");

        let dst = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(getenv_unwrap("OUT_DIR")));

        cmd.arg(format!("--buildir={}", dst.display()));

        if self.verbose {
            cmd.arg("-v");
        }

        // Cross compilation
        let host = getenv_unwrap("HOST");
        let target = getenv_unwrap("TARGET");

        let os = getenv_unwrap("CARGO_CFG_TARGET_OS");

        // Convert rust platform and arch to xmake
        let plat = self
            .get_xmake_plat(os.clone())
            .expect("unsupported rust target");

        let arch = match (
            plat.as_str(),
            getenv_unwrap("CARGO_CFG_TARGET_ARCH").as_str(),
        ) {
            ("android", a) if os == "androideabi" => match a {
                "arm" => "armeabi", // TODO Check with cc-rs if it's true
                "armv7" => "armeabi-v7a",
                a => a,
            },
            ("android", "aarch64") => "arm64-v8a",
            ("android", "i686") => "x86",
            ("appletvos", "aarch64") => "arm64",
            ("watchos", "arm64_32") => "armv7k",
            ("watchos", "armv7k") => "armv7k",
            ("iphoneos", "aarch64") => "arm64",
            ("macosx", "aarch64") => "arm64",
            ("windows", "i686") => "x86",
            ("wasm", _) => "wasm32",
            (_, "aarch64") => "arm64",
            (_, "i686") => "i386",
            (_, a) => a,
        }
        .to_string();

        if host != target {
            cmd.arg(format!("--plat={}", plat));
            if plat != "cross" {
                //cmd.arg(format!("--arch={}", arch));
            }

            if plat == "android" {
                if let Ok(ndk) = env::var("ANDROID_NDK_HOME") {
                    cmd.arg(format!("--ndk={}", ndk));
                }
                if self.cpp_link_stdlib.is_some() {
                    cmd.arg(format!(
                        "--ndk_cxxstl={}",
                        self.cpp_link_stdlib.clone().unwrap()
                    ));
                }
                cmd.arg(format!("--toolchain={}", "ndk"));
            }

            if plat == "wasm" {
                if let Ok(emscripten) = env::var("EMSCRIPTEN_HOME") {
                    cmd.arg(format!("--emsdk={}", emscripten));
                }
                cmd.arg(format!("--toolchain={}", "emcc"));
            }

            if plat == "cross" {
                let mut c_cfg = cc::Build::new();
                c_cfg
                    .cargo_metadata(false)
                    .opt_level(0)
                    .debug(false)
                    .warnings(false)
                    .host(&host)
                    .target(&target);

                // Attempt to find the cross compilation sdk
                // Let cc find it for us
                // Usually a compiler is inside bin folder and xmake wait the entire
                // sdk folder
                let compiler = c_cfg.get_compiler();
                let sdk = compiler.path().ancestors().nth(2).unwrap();

                cmd.arg(format!("--sdk={}", sdk.display()));
                cmd.arg(format!("--cross={}-{}", arch, os));
                cmd.arg(format!("--toolchain={}", "cross"));
            }
        } else {
            cmd.arg(format!("--plat={}", plat));
        }

        if plat == "windows" {
            // Static CRT
            let static_crt = self.static_crt.unwrap_or_else(|| self.get_static_crt());
            let debug = match self.get_mode() {
                // rusct doesn't support debug version of the CRT
                // "debug" => "d",
                // "releasedbg" => "d",
                _ => "",
            };

            let runtime = match static_crt {
                true => format!("--runtimes=MT{}", debug),
                false => format!("--runtimes=MD{}", debug),
            };

            cmd.arg(runtime);
        }

        // Compilation mode: release, debug...
        let mode = self.get_mode();
        cmd.arg("-m").arg(mode);

        // Option
        for (key, val) in self.options.iter() {
            let option = format!("--{}={}", key.clone(), val.clone(),);
            cmd.arg(option);
        }

        run(&mut cmd, "xmake");

        self.cache.plat = Some(plat);
        self.cache.arch = Some(arch);
    }

    /// Returns a reference to the `BuildInfo` associated with this build.
    /// <div class="warning">Note: Accessing this information before the build step will result in non-representative data.</div>
    pub fn build_info(&self) -> &BuildInfo {
        &self.cache.build_info
    }

    /// Install target in OUT_DIR.
    fn install(&mut self) -> PathBuf {
        let mut cmd = self.xmake_command();
        cmd.arg("install");

        let dst = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(getenv_unwrap("OUT_DIR")));

        cmd.arg("-o").arg(dst.clone());
        if self.verbose {
            cmd.arg("-v");
        }

        if self.target.is_some() {
            cmd.arg(self.target.clone().unwrap());
        }

        run(&mut cmd, "xmake");
        dst
    }

    fn get_build_info(&mut self) -> Option<BuildInfo> {
        let mut cmd = self.xmake_command();
        cmd.arg("lua");
        if self.verbose {
            cmd.arg("-v");
        }

        let script_file = Path::new(file!()).parent().unwrap().join("build_info.lua");
        cmd.arg(script_file);

        if let Some(output) = run(&mut cmd, "xmake") {
            return output.parse().ok();
        }
        None
    }

    fn get_static_crt(&self) -> bool {
        let feature = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or(String::new());
        if feature.contains("crt-static") {
            true
        } else {
            false
        }
    }

    /// Convert rust platform to xmake one
    fn get_xmake_plat(&self, platform: String) -> Option<String> {
        // List of xmake platform https://github.com/xmake-io/xmake/tree/master/xmake/platforms
        match platform.as_str() {
            "windows" => Some("windows".to_string()),
            "linux" => Some("linux".to_string()),
            "android" => Some("android".to_string()),
            "androideabi" => Some("android".to_string()),
            "emscripten" => Some("wasm".to_string()),
            "macos" => Some("macosx".to_string()),
            "ios" => Some("iphoneos".to_string()),
            "tvos" => Some("appletvos".to_string()),
            "fuchsia" => None,
            "solaris" => None,
            _ if getenv_unwrap("CARGO_CFG_TARGET_FAMILY") == "wasm" => Some("wasm".to_string()),
            _ => Some("cross".to_string()),
        }
    }

    /// Return xmake mode or inferred from Rust's compilation profile.
    ///
    /// * if `opt-level=0` then `debug`,
    /// * if `opt-level={1,2,3}` and:
    ///   * `debug=false` then `release`
    ///   * otherwise `releasedbg`
    /// * if `opt-level={s,z}` then `minsizerel`
    fn get_mode(&self) -> &str {
        if let Some(profile) = self.mode.as_ref() {
            profile
        } else {
            #[derive(PartialEq)]
            enum RustProfile {
                Debug,
                Release,
            }
            #[derive(PartialEq, Debug)]
            enum OptLevel {
                Debug,
                Release,
                Size,
            }

            let rust_profile = match &getenv_unwrap("PROFILE")[..] {
                "debug" => RustProfile::Debug,
                "release" | "bench" => RustProfile::Release,
                unknown => {
                    eprintln!(
                        "Warning: unknown Rust profile={}; defaulting to a release build.",
                        unknown
                    );
                    RustProfile::Release
                }
            };

            let opt_level = match &getenv_unwrap("OPT_LEVEL")[..] {
                "0" => OptLevel::Debug,
                "1" | "2" | "3" => OptLevel::Release,
                "s" | "z" => OptLevel::Size,
                unknown => {
                    let default_opt_level = match rust_profile {
                        RustProfile::Debug => OptLevel::Debug,
                        RustProfile::Release => OptLevel::Release,
                    };
                    eprintln!(
                        "Warning: unknown opt-level={}; defaulting to a {:?} build.",
                        unknown, default_opt_level
                    );
                    default_opt_level
                }
            };

            let debug_info: bool = match &getenv_unwrap("DEBUG")[..] {
                "false" => false,
                "true" => true,
                unknown => {
                    eprintln!("Warning: unknown debug={}; defaulting to `true`.", unknown);
                    true
                }
            };

            match (opt_level, debug_info) {
                (OptLevel::Debug, _) => "debug",
                (OptLevel::Release, false) => "release",
                (OptLevel::Release, true) => "releasedbg",
                (OptLevel::Size, _) => "minsizerel",
            }
        }
    }

    fn xmake_command(&mut self) -> Command {
        let mut cmd = Command::new(self.xmake_executable());
        cmd.current_dir(self.path.as_path());

        // Add envs
        for &(ref k, ref v) in self.env.iter().chain(&self.env) {
            cmd.env(k, v);
        }

        // Set the project dir env for xmake
        cmd.env("XMAKE_PROJECT_DIR", self.path.clone());
        cmd
    }

    fn xmake_executable(&mut self) -> String {
        self.getenv_os("XMAKE")
            .unwrap_or_else(|| String::from("xmake"))
    }

    fn getenv_os(&mut self, v: &str) -> Option<String> {
        if let Some(val) = self.cache.env.get(v) {
            return val.clone();
        }

        let r = env::var(v).ok();
        println!("{} = {:?}", v, r);
        self.cache.env.insert(v.to_string(), r.clone());
        r
    }
}

fn run(cmd: &mut Command, program: &str) -> Option<String> {
    println!("running: {:?}", cmd);
    let output = match cmd.output() {
        Ok(out) => out,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!(
                "failed to execute command: {}\nis `{}` not installed?",
                e, program
            ));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !output.status.success() {
        fail(&format!(
            "command did not execute successfully, got: {}",
            output.status
        ));
    }
    return String::from_utf8(output.stdout).ok();
}
/// Parses a string representation of a map of key-value pairs, where the values are
/// separated by the '|' character.
///
/// The input string is expected to be in the format "key:value1|value2|...|valueN",
/// where the values are separated by the '|' character. Any empty values are
/// filtered out.
///
fn parse_info_pairs<T: AsRef<str>>(s: T) -> HashMap<String, Vec<String>> {
    let str: String = s.as_ref().trim().to_string();
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for l in str.lines() {
        // Split between key values
        let (key, values) = l.split_once(":").unwrap();
        let v: Vec<_> = values
            .split('|')
            .map(|x| x.to_string())
            .filter(|s| !s.is_empty())
            .collect();
        map.insert(key.to_string(), v);
    }
    map
}

fn parse_field<T: FromStr>(
    map: &HashMap<String, Vec<String>>,
    field: &str,
) -> Result<T, ParseBuildInfoError> {
    map[field]
        .first()
        .ok_or(ParseBuildInfoError)
        .and_then(|v| v.parse().map_err(|_| ParseBuildInfoError))
}

fn getenv_unwrap(v: &str) -> String {
    match env::var(v) {
        Ok(s) => s,
        Err(..) => fail(&format!("environment variable `{}` not defined", v)),
    }
}

fn fail(s: &str) -> ! {
    panic!("\n{}\n\nbuild script failed, must exit now", s)
}

#[cfg(test)]
mod tests {
    use crate::{BuildInfo, Link, LinkKind};

    #[test]
    fn parse_line() {
        let expected_values: Vec<_> = vec!["value1", "value2", "value3"]
            .iter()
            .map(|x| x.to_string())
            .collect();
        let map = super::parse_info_pairs("key:value1|value2|value3");
        assert!(map.contains_key("key"));
        assert_eq!(map["key"], expected_values);
    }

    #[test]
    fn parse_line_empty_values() {
        let expected_values: Vec<_> = vec!["value1", "value2"]
            .iter()
            .map(|x| x.to_string())
            .collect();
        let map = super::parse_info_pairs("key:value1||value2");
        assert!(map.contains_key("key"));
        assert_eq!(map["key"], expected_values);
    }

    #[test]
    fn parse_build_info_missing_keys() {
        let mut s = String::new();
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");
        s.push_str("links:linkA/static|linkB/shared\n");

        let build_info: Result<BuildInfo, _> = s.parse();
        assert!(build_info.is_err());
    }

    #[test]
    fn parse_build_info_missing_kind() {
        let mut s = String::new();
        s.push_str("links:linkA|linkB\n");
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");

        let build_info: Result<BuildInfo, _> = s.parse();
        assert!(build_info.is_err());
    }

    #[test]
    fn parse_build_info_missing_info() {
        let mut s = String::new();
        s.push_str("links:linkA/static|linkB/shared\n");
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");

        let build_info: Result<BuildInfo, _> = s.parse();
        assert!(build_info.is_err());
    }

    #[test]
    fn parse_build_info() {
        let expected_links = vec![
            Link::new("linkA", LinkKind::Static),
            Link::new("linkB", LinkKind::Dynamic),
        ];
        let expected_directories = vec!["path/to/libA", "path/to/libB", "path\\to\\libC"];
        let expected_cxx = true;
        let expected_stl = false;

        let mut s = String::new();
        s.push_str("cxx_used:true\n");
        s.push_str("stl_used:false\n");
        s.push_str("links:linkA/static|linkB/shared\n");
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");

        let build_info: BuildInfo = s.parse().unwrap();

        assert_eq!(build_info.links(), &expected_links);
        assert_eq!(build_info.directories(), &expected_directories);
        assert_eq!(build_info.use_cxx(), expected_cxx);
        assert_eq!(build_info.use_stl(), expected_stl);
    }
}
