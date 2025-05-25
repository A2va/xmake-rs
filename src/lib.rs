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
//! xmake = "0.3.2"
//! ```
//!
//! ## Examples
//!
//! ```no_run
//! use xmake;
//!
//! // Builds the project in the directory located in `libfoo`, and link it
//! xmake::build("libfoo");
//! ```
//!
//! ```no_run
//! use xmake::Config;
//!
//! Config::new("libfoo")
//!        .option("bar", "true")
//!        .env("XMAKE", "path/to/xmake")
//!        .build();
//! ```
#![deny(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

// The version of xmake that is required for this crate to work.
// https://github.com/xmake-io/xmake/releases/tag/v2.9.6
const XMAKE_MINIMUM_VERSION: Version = Version::new(2, 9, 6);

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
    /// The library is a framework, like [`LinkKind::System`], it is provided by the operating system but used only on macos.
    Framework,
    /// The library is unknown, meaning its kind could not be determined.
    Unknown,
}

/// Represents the source when querying some information from [`BuildInfo`].
pub enum Source {
    /// Coming from an xmake target
    Target,
    /// Coming from an xmake package
    Package,
    /// Both of them
    Both,
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
    linkdirs: Vec<PathBuf>,
    /// The individual linked libraries.
    links: Vec<Link>,
    /// All the includirs coming from the packages
    includedirs_package: HashMap<String, Vec<PathBuf>>,
    /// All the includirs coming from the targets
    includedirs_target: HashMap<String, Vec<PathBuf>>,
    /// Whether the build uses the C++.
    use_cxx: bool,
    /// Whether the build uses the C++ standard library.
    use_stl: bool,
}

/// Represents errors that can occur when parsing a string to it's `BuildInfo` representation.
#[derive(Debug, PartialEq, Eq)]
pub enum ParsingError {
    /// Given kind did not match any of the `LinkKind` variants.
    InvalidKind,
    /// Missing at least one key to construct `BuildInfo`.
    MissingKey,
    /// Link string is malformed.
    MalformedLink,
    /// Multiple values when it's not supposed to
    MultipleValues,
    /// Error when converting string a type
    ParseError,
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
    pub fn linkdirs(&self) -> &[PathBuf] {
        &self.linkdirs
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

    /// Retrieves the include directories for the specific target/package given it's name.
    /// If `*` is given as a name all the includedirs will be returned.
    pub fn includedirs<S: AsRef<str>>(&self, source: Source, name: S) -> Vec<PathBuf> {
        let name = name.as_ref();
        let mut result = Vec::new();

        let sources = match source {
            Source::Target => vec![&self.includedirs_target],
            Source::Package => vec![&self.includedirs_package],
            Source::Both => vec![&self.includedirs_target, &self.includedirs_package],
        };

        for map in sources {
            if name == "*" {
                result.extend(map.values().cloned().flatten());
            } else if let Some(dirs) = map.get(name) {
                result.extend(dirs.clone());
            }
        }

        result
    }
}

impl FromStr for LinkKind {
    type Err = ParsingError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "static" => Ok(LinkKind::Static),
            "shared" => Ok(LinkKind::Dynamic),
            "system" => Ok(LinkKind::System),
            "framework" => Ok(LinkKind::Framework),
            "unknown" => Ok(LinkKind::Unknown),
            _ => Err(ParsingError::InvalidKind),
        }
    }
}

impl FromStr for Link {
    type Err = ParsingError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const NUMBER_OF_PARTS: usize = 2;

        let parts: Vec<_> = s.split("/").collect();
        if parts.len() != NUMBER_OF_PARTS {
            return Err(ParsingError::MalformedLink);
        }

        let kind_result: LinkKind = parts[1].parse()?;
        Ok(Link {
            name: parts[0].to_string(),
            kind: kind_result,
        })
    }
}

impl FromStr for BuildInfo {
    type Err = ParsingError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let map = parse_info_pairs(s);

        let directories: Vec<PathBuf> = parse_field(&map, "linkdirs")?;
        let links: Vec<Link> = parse_field(&map, "links")?;

        let use_cxx: bool = parse_field(&map, "cxx_used")?;
        let use_stl: bool = parse_field(&map, "stl_used")?;

        let packages = subkeys_of(&map, "includedirs_package");
        let mut includedirs_package = HashMap::new();
        for package in packages {
            let dirs: Vec<PathBuf> = parse_field(&map, format!("includedirs_package.{}", package))?;
            includedirs_package.insert(package.to_string(), dirs);
        }

        let targets = subkeys_of(&map, "includedirs_target");
        let mut includedirs_target = HashMap::new();
        for target in targets {
            let dirs: Vec<PathBuf> = parse_field(&map, format!("includedirs_target.{}", target))?;
            includedirs_target.insert(target.to_string(), dirs);
        }

        Ok(BuildInfo {
            linkdirs: directories,
            links: links,
            use_cxx: use_cxx,
            use_stl: use_stl,
            includedirs_package: includedirs_package,
            includedirs_target: includedirs_target,
        })
    }
}

#[derive(Default)]
struct ConfigCache {
    build_info: BuildInfo,
    plat: Option<String>,
    arch: Option<String>,
    xmake_version: Option<Version>,
    env: HashMap<String, Option<String>>,
}

impl ConfigCache {
    /// Returns the platform string for this configuration.
    /// Panic if the config has not been done yet
    fn plat(&self) -> &String {
        return self.plat.as_ref().unwrap();
    }

    /// Returns the architecture string for this configuration.
    /// Panic if the config has not been done yet
    fn arch(&self) -> &String {
        return self.arch.as_ref().unwrap();
    }
}

/// Builder style configuration for a pending XMake build.
pub struct Config {
    path: PathBuf,
    targets: Option<String>,
    verbose: bool,
    auto_link: bool,
    out_dir: Option<PathBuf>,
    mode: Option<String>,
    options: Vec<(String, String)>,
    env: Vec<(String, String)>,
    static_crt: Option<bool>,
    runtimes: Option<String>,
    no_stl_link: bool,
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
/// // Builds the project in the directory located in `libfoo`, and link it
/// xmake::build("libfoo");
/// ```
///
pub fn build<P: AsRef<Path>>(path: P) {
    Config::new(path.as_ref()).build()
}

impl Config {
    /// Creates a new blank set of configuration to build the project specified
    /// at the path `path`.
    pub fn new<P: AsRef<Path>>(path: P) -> Config {
        Config {
            path: env::current_dir().unwrap().join(path),
            targets: None,
            verbose: false,
            auto_link: true,
            out_dir: None,
            mode: None,
            options: Vec::new(),
            env: Vec::new(),
            static_crt: None,
            runtimes: None,
            no_stl_link: false,
            cache: ConfigCache::default(),
        }
    }

    /// Sets the xmake targets for this compilation.
    /// Note: This is different from rust target (os and arch).
    /// ```
    /// use xmake::Config;
    /// let mut config = xmake::Config::new("libfoo");
    /// config.targets("foo");
    /// config.targets("foo,bar");
    /// config.targets(["foo", "bar"]); // You can also pass a Vec<String> or Vec<&str>
    /// ```
    pub fn targets<T: CommaSeparated>(&mut self, targets: T) -> &mut Config {
        self.targets = Some(targets.as_comma_separated());
        self
    }

    /// Sets verbose output.
    pub fn verbose(&mut self, value: bool) -> &mut Config {
        self.verbose = value;
        self
    }

    /// Configures if targets and their dependencies should be linked.
    /// <div class="warning">Without configuring `no_stl_link`, the C++ standard library will be linked, if used in the project. </div>
    /// This option defaults to `true`.
    pub fn auto_link(&mut self, value: bool) -> &mut Config {
        self.auto_link = value;
        self
    }

    /// Configures if the C++ standard library should be linked.
    ///
    /// This option defaults to `false`.
    /// If false and no runtimes options is set, the runtime flag passed to xmake configuration will be not set at all.
    pub fn no_stl_link(&mut self, value: bool) -> &mut Config {
        self.no_stl_link = value;
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

    /// Sets the runtimes to use for this compilation.
    ///
    /// This method takes a collection of runtime names, which will be passed to
    /// the `xmake` command during the build process. The runtimes specified here
    /// will be used to determine the appropriate C++ standard library to link
    /// against.
    /// Common values:
    /// - `MT`
    /// - `MTd`
    /// - `MD`
    /// - `MDd`
    /// - `c++_static`
    /// - `c++_shared`
    /// - `stdc++_static`
    /// - `stdc++_shared`
    /// - `gnustl_static`
    /// - `gnustl_shared`
    /// - `stlport_shared`
    /// - `stlport_static`
    /// ```
    /// use xmake::Config;
    /// let mut config = xmake::Config::new("libfoo");
    /// config.runtimes("MT,c++_static");
    /// config.runtimes(["MT", "c++_static"]); // You can also pass a Vec<String> or Vec<&str>
    /// ```
    pub fn runtimes<T: CommaSeparated>(&mut self, runtimes: T) -> &mut Config {
        self.runtimes = Some(runtimes.as_comma_separated());
        self
    }

    /// Run this configuration, compiling the library with all the configured
    /// options.
    ///
    /// This will run both the configuration command as well as the
    /// command to build the library.
    pub fn build(&mut self) {
        self.config();

        let mut cmd = self.xmake_command();

        // In case of xmake is waiting to download something
        cmd.arg("--yes");

        if let Some(targets) = &self.targets {
            // :: is used to handle namespaces in xmake but it interferes with the env separator
            // on linux, so we use a different separator
            cmd.env("XMAKERS_TARGETS", targets.replace("::", "||"));
        }

        cmd.run_script("build.lua");

        if let Some(info) = self.get_build_info() {
            self.cache.build_info = info;
        }

        if self.auto_link {
            self.link();
        }
    }

    /// Returns a reference to the `BuildInfo` associated with this build.
    /// <div class="warning">Note: Accessing this information before the build step will result in non-representative data.</div>
    pub fn build_info(&self) -> &BuildInfo {
        &self.cache.build_info
    }

    // Run the configuration with all the configured
    /// options.
    fn config(&mut self) {
        self.check_version();

        let mut cmd = self.xmake_command();
        cmd.task("config");

        // In case of xmake is waiting to download something
        cmd.arg("--yes");

        let dst = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(getenv_unwrap("OUT_DIR")));

        cmd.arg(format!("--buildir={}", dst.display()));

        // Cross compilation
        let host = getenv_unwrap("HOST");
        let target = getenv_unwrap("TARGET");

        let os = getenv_unwrap("CARGO_CFG_TARGET_OS");

        let plat = self.get_xmake_plat();
        cmd.arg(format!("--plat={}", plat));

        if host != target {
            let arch = self.get_xmake_arch();
            cmd.arg(format!("--arch={}", arch));

            if plat == "android" {
                if let Ok(ndk) = env::var("ANDROID_NDK_HOME") {
                    cmd.arg(format!("--ndk={}", ndk));
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
                // Usually a compiler is inside bin folder and xmake expect the entire
                // sdk folder
                let compiler = c_cfg.get_compiler();
                let sdk = compiler.path().ancestors().nth(2).unwrap();

                cmd.arg(format!("--sdk={}", sdk.display()));
                cmd.arg(format!("--cross={}-{}", arch, os));
                cmd.arg(format!("--toolchain={}", "cross"));
            }
        }

        // Configure the runtimes
        if let Some(runtimes) = &self.runtimes {
            cmd.arg(format!("--runtimes={}", runtimes));
        } else if let Some(runtimes) = self.get_runtimes() {
            if !self.no_stl_link {
                cmd.arg(format!("--runtimes={}", runtimes));
            }
        }

        // Compilation mode: release, debug...
        let mode = self.get_mode();
        cmd.arg("-m").arg(mode);

        // Option
        for (key, val) in self.options.iter() {
            let option = format!("--{}={}", key.clone(), val.clone(),);
            cmd.arg(option);
        }

        cmd.run();
    }

    fn link(&mut self) {
        let dst = self.install();
        let plat = self.get_xmake_plat();

        let build_info = &mut self.cache.build_info;

        for directory in build_info.linkdirs() {
            // Reference: https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-link-search
            println!("cargo:rustc-link-search=all={}", directory.display());
        }

        // Special link search path for dynamic libraries, because
        // the path are appended to the dynamic library search path environment variable
        // only if there are within OUT_DIR
        let linux_shared_libs_folder = dst.join("lib");
        println!(
            "cargo:rustc-link-search=native={}",
            linux_shared_libs_folder.display()
        );
        println!(
            "cargo:rustc-link-search=native={}",
            dst.join("bin").display()
        );

        build_info.linkdirs.push(linux_shared_libs_folder.clone());
        build_info.linkdirs.push(dst.join("bin"));

        let mut shared_libs = HashSet::new();

        for link in build_info.links() {
            match link.kind() {
                LinkKind::Static => println!("cargo:rustc-link-lib=static={}", link.name()),
                LinkKind::Dynamic => {
                    println!("cargo:rustc-link-lib=dylib={}", link.name());
                    shared_libs.insert(link.name());
                }
                LinkKind::Framework if plat == "macosx" => {
                    println!("cargo:rustc-link-lib=framework={}", link.name())
                }
                // For rust, framework type is only for macosx but can be used on multiple system in xmake
                // so fallback to the system libraries case
                LinkKind::System | LinkKind::Framework => {
                    println!("cargo:rustc-link-lib={}", link.name())
                }
                // Let try cargo handle the rest
                LinkKind::Unknown => println!("cargo:rustc-link-lib={}", link.name()),
            }
        }

        // In some cases, xmake does not include all the shared libraries in the link cmd (for example, in the sht-shf-shb test),
        // leading to build failures on the rust side because it expected to link them, so the solution is to fetches all the libs from the install directory.
        // Since I cannot know the real order of the links, this can cause some problems on some projects.
        if plat == "linux" && linux_shared_libs_folder.exists() {
            let files = std::fs::read_dir(dst.join("lib")).unwrap();
            for entry in files {
                if let Ok(file) = entry {
                    let file_name = file.file_name();
                    let file_name = file_name.to_str().unwrap();
                    if file_name.ends_with(".so") || file_name.matches(r"\.so\.\d+").count() > 0 {
                        if let Some(lib_name) = file_name.strip_prefix("lib") {
                            let name = if let Some(dot_pos) = lib_name.find(".so") {
                                &lib_name[..dot_pos]
                            } else {
                                lib_name
                            };

                            if !shared_libs.contains(name) {
                                println!("cargo:rustc-link-lib=dylib={}", name);
                            }
                        }
                    }
                }
            }
        }

        if !self.no_stl_link && self.build_info().use_stl() {
            if let Some(runtimes) = &self.runtimes {
                let plat = self.cache.plat();

                let stl: Option<&[&str]> = match plat.as_str() {
                    "linux" => {
                        Some(&["c++_static", "c++_shared", "stdc++_static", "stdc++_shared"])
                    }
                    "android" => Some(&[
                        "c++_static",
                        "c++_shared",
                        "gnustl_static",
                        "gnustl_shared",
                        "stlport_static",
                        "stlport_shared",
                    ]),
                    _ => None,
                };

                if let Some(stl) = stl {
                    // Try to match the selected runtime with the available runtimes
                    for runtime in runtimes.split(",") {
                        if stl.contains(&runtime) {
                            let (name, _) = runtime.split_once("_").unwrap();
                            let kind = match runtime.contains("static") {
                                true => "static",
                                false => "dylib",
                            };
                            println!(r"cargo:rustc-link-lib={}={}", kind, name);
                            break;
                        }
                    }
                }
            } else {
                if let Some(runtime) = self.get_runtimes() {
                    let (name, _) = runtime.split_once("_").unwrap();
                    let kind = match runtime.contains("static") {
                        true => "static",
                        false => "dylib",
                    };
                    println!(r"cargo:rustc-link-lib={}={}", kind, name);
                }
            }
        }
    }

    /// Install target in OUT_DIR.
    fn install(&mut self) -> PathBuf {
        let mut cmd = self.xmake_command();

        let dst = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(getenv_unwrap("OUT_DIR")));

        cmd.env("XMAKERS_INSTALL_DIR", dst.clone());
        cmd.run_script("install.lua");
        dst
    }

    fn get_build_info(&mut self) -> Option<BuildInfo> {
        let mut cmd = self.xmake_command();

        if let Some(targets) = &self.targets {
            // :: is used to handle namespaces in xmake but it interferes with the env separator
            // on linux, so use a different separator
            cmd.env("XMAKERS_TARGETS", targets.replace("::", "||"));
        }

        if let Some(output) = cmd.run_script("build_info.lua") {
            return output.parse().ok();
        }
        None
    }

    fn get_static_crt(&self) -> bool {
        return self.static_crt.unwrap_or_else(|| {
            let feature = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or(String::new());
            if feature.contains("crt-static") {
                true
            } else {
                false
            }
        });
    }

    // In case no runtimes has been set, get one
    fn get_runtimes(&mut self) -> Option<String> {
        // These runtimes may not be the most appropriate for each platform, but
        // taken the GNU standard libary is the most common one on linux, and same for
        // the clang equivalent on android.
        // TODO Explore which runtimes is more approriate for macosx
        let static_crt = self.get_static_crt();
        let platform = self.get_xmake_plat();

        let kind = match static_crt {
            true => "static",
            false => "shared",
        };

        match platform.as_str() {
            "linux" => Some(format!("stdc++_{}", kind)),
            "android" => Some(format!("c++_{}", kind)),
            "windows" => {
                let msvc_runtime = if static_crt { "MT" } else { "MD" };
                Some(msvc_runtime.to_owned())
            }
            _ => None,
        }
    }

    /// Convert rust platform to xmake one
    fn get_xmake_plat(&mut self) -> String {
        if let Some(ref plat) = self.cache.plat {
            return plat.clone();
        }

        // List of xmake platform https://github.com/xmake-io/xmake/tree/master/xmake/platforms
        // Rust targets: https://doc.rust-lang.org/rustc/platform-support.html
        let plat = match self.getenv_os("CARGO_CFG_TARGET_OS").unwrap().as_str() {
            "windows" => Some("windows"),
            "linux" => Some("linux"),
            "android" => Some("android"),
            "androideabi" => Some("android"),
            "emscripten" => Some("wasm"),
            "macos" => Some("macosx"),
            "ios" => Some("iphoneos"),
            "tvos" => Some("appletvos"),
            "fuchsia" => None,
            "solaris" => None,
            _ if getenv_unwrap("CARGO_CFG_TARGET_FAMILY") == "wasm" => Some("wasm"),
            _ => Some("cross"),
        }
        .expect("unsupported rust target");

        self.cache.plat = Some(plat.to_string());
        self.cache.plat.clone().unwrap()
    }

    fn get_xmake_arch(&mut self) -> String {
        if let Some(ref arch) = self.cache.arch {
            return arch.clone();
        }

        // List rust targets with rustc --print target-list
        let os = self.getenv_os("CARGO_CFG_TARGET_OS").unwrap();
        let target_arch = self.getenv_os("CARGO_CFG_TARGET_ARCH").unwrap();
        let plat = self.get_xmake_plat();

        // From v2.9.9 (not released) onwards, XMake used arm64 instead of arm64-v8a
        let arm64_changes = self
            .cache
            .xmake_version
            .as_ref()
            .unwrap_or(&XMAKE_MINIMUM_VERSION)
            < &Version::new(2, 9, 9);

        let arch = match (plat.as_str(), target_arch.as_str()) {
            ("android", a) if os == "androideabi" => match a {
                "arm" => "armeabi", // TODO Check with cc-rs if it's true
                "armv7" => "armeabi-v7a",
                a => a,
            },
            ("android", "aarch64") => "arm64-v8a",
            ("android", "i686") => "x86",
            ("linux", "loongarch64") => "loong64",
            // From v2.9.9 (not released) onwards, XMake used arm64 instead of arm64-v8a
            ("linux", "aarch64") if arm64_changes => "arm64-v8a",
            ("watchos", "arm64_32") => "armv7k",
            ("watchos", "armv7k") => "armv7k",
            ("iphoneos", "aarch64") => "arm64",
            ("macosx", "aarch64") => "arm64",
            ("windows", "i686") => "x86",
            (_, "aarch64") => "arm64",
            (_, "i686") => "i386",
            (_, a) => a,
        }
        .to_string();

        self.cache.arch = Some(arch);
        self.cache.arch.clone().unwrap()
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

    fn check_version(&mut self) {
        let version = Version::from_command();
        if version.is_none() {
            println!("cargo:warning=xmake version could not be determined, it might not work");
            return;
        }

        let version = version.unwrap();
        if version < XMAKE_MINIMUM_VERSION {
            panic!(
                "xmake version {:?} is too old, please update to at least {:?}",
                version, XMAKE_MINIMUM_VERSION
            );
        }
        self.cache.xmake_version = Some(version);
    }

    fn xmake_command(&mut self) -> XmakeCommand {
        let mut cmd = XmakeCommand::new();

        // Add envs
        for &(ref k, ref v) in self.env.iter().chain(&self.env) {
            cmd.env(k, v);
        }

        if self.verbose {
            cmd.verbose(true);
        }

        cmd.project_dir(self.path.as_path());

        cmd
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

trait CommaSeparated {
    fn as_comma_separated(self) -> String;
}

impl<const N: usize> CommaSeparated for [&str; N] {
    fn as_comma_separated(self) -> String {
        self.join(",")
    }
}

impl CommaSeparated for Vec<String> {
    fn as_comma_separated(self) -> String {
        self.join(",")
    }
}

impl CommaSeparated for Vec<&str> {
    fn as_comma_separated(self) -> String {
        self.join(",")
    }
}

impl CommaSeparated for String {
    fn as_comma_separated(self) -> String {
        self
    }
}

impl CommaSeparated for &str {
    fn as_comma_separated(self) -> String {
        self.to_string()
    }
}

/// Parses a string representation of a map of key-value pairs, where the values are
/// separated by the '|' character.
///
/// The input string is expected to be in the format "key:value1|value2|...|valueN",
/// where the values are separated by the '|' character. Any empty values are
/// filtered out.
///
fn parse_info_pairs<S: AsRef<str>>(s: S) -> HashMap<String, Vec<String>> {
    let str: String = s.as_ref().trim().to_string();
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for l in str.lines() {
        // Split between key values
        if let Some((key, values)) = l.split_once(":") {
            let v: Vec<_> = values
                .split('|')
                .map(|x| x.to_string())
                .filter(|s| !s.is_empty())
                .collect();
            map.insert(key.to_string(), v);
        }
    }
    map
}

fn subkeys_of<S: AsRef<str>>(map: &HashMap<String, Vec<String>>, main_key: S) -> Vec<&str> {
    let main_key = main_key.as_ref();
    let prefix = format!("{main_key}.");
    map.keys().filter_map(|k| k.strip_prefix(&prefix)).collect()
}

// This trait may be replaced by the unstable auto trait feature
// References:
// https://users.rust-lang.org/t/how-to-exclude-a-type-from-generic-trait-implementation/26156/9
// https://doc.rust-lang.org/beta/unstable-book/language-features/auto-traits.html
// https://doc.rust-lang.org/beta/unstable-book/language-features/negative-impls.html
trait DirectParse {}

// Implement for all primitive types that should use the scalar implementation
impl DirectParse for bool {}
impl DirectParse for u32 {}
impl DirectParse for String {}

trait ParseField<T> {
    fn parse_field<S: AsRef<str>>(
        map: &HashMap<String, Vec<String>>,
        field: S,
    ) -> Result<T, ParsingError>;
}

// Only implement for types that implement DirectParse
impl<T> ParseField<T> for T
where
    T: FromStr + DirectParse,
{
    fn parse_field<S: AsRef<str>>(
        map: &HashMap<String, Vec<String>>,
        field: S,
    ) -> Result<T, ParsingError> {
        let field = field.as_ref();
        let values = map.get(field).ok_or(ParsingError::MissingKey)?;
        if values.len() > 1 {
            return Err(ParsingError::MultipleValues);
        }

        let parsed: Vec<T> = values
            .iter()
            .map(|s| s.parse::<T>().map_err(|_| ParsingError::ParseError))
            .collect::<Result<Vec<T>, ParsingError>>()?;
        parsed.into_iter().next().ok_or(ParsingError::MissingKey)
    }
}

// Vector implementation remains unchanged
impl<T> ParseField<Vec<T>> for Vec<T>
where
    T: FromStr,
{
    fn parse_field<S: AsRef<str>>(
        map: &HashMap<String, Vec<String>>,
        field: S,
    ) -> Result<Vec<T>, ParsingError> {
        let field = field.as_ref();
        let values = map.get(field).ok_or(ParsingError::MissingKey)?;
        values
            .iter()
            .map(|s| s.parse::<T>().map_err(|_| ParsingError::ParseError))
            .collect::<Result<Vec<T>, ParsingError>>()
    }
}

fn parse_field<T, S: AsRef<str>>(
    map: &HashMap<String, Vec<String>>,
    field: S,
) -> Result<T, ParsingError>
where
    T: ParseField<T>,
{
    T::parse_field(map, field)
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

struct XmakeCommand {
    verbose: bool,
    diagnosis: bool,
    raw_output: bool,
    command: Command,
    args: Vec<std::ffi::OsString>,
    task: Option<String>,
    project_dir: Option<PathBuf>,
}

impl XmakeCommand {
    /// Create a new XmakeCommand instance.
    fn new() -> Self {
        let mut command = Command::new(Self::xmake_executable());
        command.env("XMAKE_THEME", "plain");
        Self {
            verbose: false,
            diagnosis: false,
            raw_output: false,
            task: None,
            command: command,
            args: Vec::new(),
            project_dir: None,
        }
    }

    fn xmake_executable() -> String {
        env::var("XMAKE").unwrap_or(String::from("xmake"))
    }

    /// Same as [`Command::arg`]
    pub fn arg<S: AsRef<std::ffi::OsStr>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Same as [`Command::env`]
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<std::ffi::OsStr>,
        V: AsRef<std::ffi::OsStr>,
    {
        self.command.env(key, val);
        self
    }

    /// Enable/disable verbose mode of xmake (default is false).
    /// Correspond to the -v flag.
    pub fn verbose(&mut self, value: bool) -> &mut Self {
        self.verbose = value;
        self
    }

    // Enable/disable diagnosis mode of xmake (default is false).
    /// Correspond to the -D flag.
    pub fn diagnosis(&mut self, value: bool) -> &mut Self {
        self.diagnosis = value;
        self
    }

    /// Sets the xmake tasks to run.
    pub fn task<S: Into<String>>(&mut self, task: S) -> &mut Self {
        self.task = Some(task.into());
        self
    }

    /// Sets the project directory.
    pub fn project_dir<P: AsRef<Path>>(&mut self, project_dir: P) -> &mut Self {
        use crate::path_clean::PathClean;
        self.project_dir = Some(project_dir.as_ref().to_path_buf().clean());
        self
    }

    /// Controls whether to capture raw, unfiltered command output (default is false).
    ///
    /// When enabled (true):
    /// - All command output is captured and returned
    ///
    /// When disabled (false, default):
    /// - Only captures output between special markers (`__xmakers_start__` and `__xmakers_end__`)
    /// - Filters out diagnostic and setup information
    ///
    /// This setting is passed to the [`run`] function to control output processing.
    pub fn raw_output(&mut self, value: bool) -> &mut Self {
        self.raw_output = value;
        self
    }

    /// Run the command and return the output as a string.
    /// Alias of [`run`]
    pub fn run(&mut self) -> Option<String> {
        if let Some(task) = &self.task {
            self.command.arg(task);
        }

        if self.verbose {
            self.command.arg("-v");
        }
        if self.diagnosis {
            self.command.arg("-D");
        }

        if let Some(project_dir) = &self.project_dir {
            // Project directory are evaluated like this:
            // 1. The Given Command Argument
            // 2. The Environment Variable: XMAKE_PROJECT_DIR
            // 3. The Current Directory
            //
            // The env doesn't work here because it is global, so it breaks
            // packages. Just to be sure set both argument and current directory.
            let project_dir = project_dir.as_path();
            self.command.current_dir(project_dir);
            self.command.arg("-P").arg(project_dir);
        }

        for arg in &self.args {
            self.command.arg(arg);
        }
        run(&mut self.command, "xmake", self.raw_output)
    }

    /// Execute a lua script, located in the src folder of this crate.
    /// Note that this method overide any previously configured taks to be `lua`.
    pub fn run_script<S: AsRef<str>>(&mut self, script: S) -> Option<String> {
        let script = script.as_ref();

        // Get absolute path to the crate root
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let script_file = crate_root.join("src").join(script);

        // Script to execute are positional argument so always last
        self.args.push(script_file.into());
        self.task("lua"); // For the task to be lua

        self.run()
    }
}

fn run(cmd: &mut Command, program: &str, raw_output: bool) -> Option<String> {
    println!("running: {:?}", cmd);
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!(
                "failed to execute command: {}\nis `{}` not installed?",
                e, program
            ));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };

    let mut output = String::new();
    let mut take_output = false;

    // Read stdout in real-time
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                // Print stdout for logging
                println!("{}", line);

                take_output &= !line.starts_with("__xmakers_start__");
                if take_output || raw_output {
                    output.push_str(line.as_str());
                    output.push('\n');
                }
                take_output |= line.starts_with("__xmakers_start__");
            }
        }
    }

    // Wait for the command to complete
    let status = child.wait().expect("failed to wait on child process");

    if !status.success() {
        fail(&format!(
            "command did not execute successfully, got: {}",
            status
        ));
    }

    Some(output)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        // As of v2.9.5, the format of the version output is "xmake v2.9.5+dev.478972cd9, A cross-platform build utility based on Lua".
        // ```
        // $ xmake --version
        // xmake v2.9.8+HEAD.13fc39238, A cross-platform build utility based on Lua
        // Copyright (C) 2015-present Ruki Wang, tboox.org, xmake.io
        //                         _
        //    __  ___ __  __  __ _| | ______
        //    \ \/ / |  \/  |/ _  | |/ / __ \
        //     >  <  | \__/ | /_| |   <  ___/
        //    /_/\_\_|_|  |_|\__ \|_|\_\____|
        //                          by ruki, xmake.io
        //
        //     point_right  Manual: https://xmake.io/#/getting_started
        //     pray  Donate: https://xmake.io/#/sponsor
        // ```
        let version = s.lines().next()?.strip_prefix("xmake v")?;
        let mut parts = version.splitn(2, '+'); // split at the '+' to separate the version and commit

        let version_part = parts.next()?;
        // Get commit and branch
        // let commit_part = parts.next().unwrap_or(""); // if there's no commit part, use an empty string
        // let mut commit_parts = commit_part.splitn(2, '.'); // split commit part to get branch and commit hash
        // let branch = commit_parts.next().unwrap_or("");
        // let commit = commit_parts.next().unwrap_or("");

        let mut digits = version_part.splitn(3, '.');
        let major = digits.next()?.parse::<u32>().ok()?;
        let minor = digits.next()?.parse::<u32>().ok()?;
        let patch = digits.next()?.parse::<u32>().ok()?;

        Some(Version::new(major, minor, patch))
    }

    fn from_command() -> Option<Self> {
        let output = XmakeCommand::new()
            .raw_output(true)
            .arg("--version")
            .run()?;
        Self::parse(output.as_str())
    }
}

mod path_clean {
    // Taken form the path-clean crate.
    // Crates.io: https://crates.io/crates/path-clean
    // GitHub: https://github.com/danreeves/path-clean

    use std::path::{Component, Path, PathBuf};
    pub(super) trait PathClean {
        fn clean(&self) -> PathBuf;
    }

    impl PathClean for Path {
        fn clean(&self) -> PathBuf {
            clean(self)
        }
    }

    impl PathClean for PathBuf {
        fn clean(&self) -> PathBuf {
            clean(self)
        }
    }

    pub(super) fn clean<P>(path: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        let mut out = Vec::new();

        for comp in path.as_ref().components() {
            match comp {
                Component::CurDir => (),
                Component::ParentDir => match out.last() {
                    Some(Component::RootDir) => (),
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    None
                    | Some(Component::CurDir)
                    | Some(Component::ParentDir)
                    | Some(Component::Prefix(_)) => out.push(comp),
                },
                comp => out.push(comp),
            }
        }

        if !out.is_empty() {
            out.iter().collect()
        } else {
            PathBuf::from(".")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, vec};

    use crate::{
        parse_field, parse_info_pairs, subkeys_of, BuildInfo, Link, LinkKind, ParsingError, Source,
    };

    fn to_set<T: std::cmp::Eq + std::hash::Hash>(vec: Vec<T>) -> std::collections::HashSet<T> {
        vec.into_iter().collect()
    }

    #[test]
    fn parse_line() {
        let expected_values: Vec<_> = ["value1", "value2", "value3"].map(String::from).to_vec();
        let map = parse_info_pairs("key:value1|value2|value3");
        assert!(map.contains_key("key"));
        assert_eq!(map["key"], expected_values);
    }

    #[test]
    fn parse_line_empty_values() {
        let expected_values: Vec<_> = ["value1", "value2"].map(String::from).to_vec();
        let map = parse_info_pairs("key:value1||value2");
        assert!(map.contains_key("key"));
        assert_eq!(map["key"], expected_values);
    }

    #[test]
    fn parse_field_multiple_values() {
        let map = parse_info_pairs("key:value1|value2|value3");
        let result: Result<String, _> = parse_field(&map, "key");
        assert!(map.contains_key("key"));
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), ParsingError::MultipleValues);
    }

    #[test]
    fn parse_with_subkeys() {
        let map = parse_info_pairs("main:value\nmain.subkey:value1|value2|value3\nmain.sub2:vv");
        let subkeys = to_set(subkeys_of(&map, "main"));
        assert_eq!(subkeys, to_set(vec!["sub2", "subkey"]));
    }

    #[test]
    fn parse_build_info_missing_key() {
        let mut s = String::new();
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");
        s.push_str("links:linkA/static|linkB/shared\n");

        let build_info: Result<BuildInfo, _> = s.parse();
        assert!(build_info.is_err());
        assert_eq!(build_info.err().unwrap(), ParsingError::MissingKey);
    }

    #[test]
    fn parse_build_info_missing_kind() {
        let mut s = String::new();
        s.push_str("cxx_used:true\n");
        s.push_str("stl_used:false\n");
        s.push_str("links:linkA|linkB\n");
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");

        let build_info: Result<BuildInfo, _> = s.parse();
        assert!(build_info.is_err());

        // For now the returned error is not MalformedLink because map_err in parse_field shallow
        // all the errors which are converted to ParsingError::ParseError
        // assert_eq!(build_info.err().unwrap(), ParsingError::MalformedLink);
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
        let expected_links = [
            Link::new("linkA", LinkKind::Static),
            Link::new("linkB", LinkKind::Dynamic),
        ];
        let expected_directories = ["path/to/libA", "path/to/libB", "path\\to\\libC"]
            .map(PathBuf::from)
            .to_vec();
    
        let expected_includedirs_package_a = to_set(["includedir/a", "includedir\\aa"]
            .map(PathBuf::from)
            .to_vec());
        let expected_includedirs_package_b = to_set(["includedir/bb", "includedir\\b"]
            .map(PathBuf::from)
            .to_vec());

        let expected_includedirs_target_c = to_set(["includedir/c"].map(PathBuf::from).to_vec());

        let expected_includedirs_both_greedy = to_set([
            "includedir/c",
            "includedir/bb",
            "includedir\\b",
            "includedir/a",
            "includedir\\aa",
        ]
        .map(PathBuf::from)
        .to_vec());

        let expected_cxx = true;
        let expected_stl = false;

        let mut s = String::new();
        s.push_str("cxx_used:true\n");
        s.push_str("stl_used:false\n");
        s.push_str("links:linkA/static|linkB/shared\n");
        s.push_str("linkdirs:path/to/libA|path/to/libB|path\\to\\libC\n");
        s.push_str("includedirs_package.a:includedir/a|includedir\\aa\n");
        s.push_str("includedirs_package.b:includedir/bb|includedir\\b\n");
        s.push_str("includedirs_target.c:includedir/c");

        let build_info: BuildInfo = s.parse().unwrap();

        assert_eq!(build_info.links(), &expected_links);
        assert_eq!(build_info.linkdirs(), &expected_directories);
        assert_eq!(build_info.use_cxx(), expected_cxx);
        assert_eq!(build_info.use_stl(), expected_stl);

        assert_eq!(
            to_set(build_info.includedirs(Source::Package, "a")),
            expected_includedirs_package_a
        );
        assert_eq!(
            to_set(build_info.includedirs(Source::Package, "b")),
            expected_includedirs_package_b
        );
        assert_eq!(
            to_set(build_info.includedirs(Source::Target, "c")),
            expected_includedirs_target_c
        );
        assert_eq!(
            to_set(build_info.includedirs(Source::Both, "*")),
            expected_includedirs_both_greedy
        );
    }
}
