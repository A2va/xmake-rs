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

use std::collections::HashMap;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

// The version of xmake that is required for this crate to work.
// https://github.com/xmake-io/xmake/wiki/Xmake-v2.8.7-released
const XMAKE_MINIMUM_VERSION: Version = Version::new(2, 8, 7);

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
    /// The library is a framework, like [`System`]: self::LinkKind#variant.System it is provided by the operating system but used only on macos.
    Framework,
    /// The library is unknown, meaning its kind could not be determined.
    Unknown,
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
    linkdirs: Vec<String>,
    /// The individual linked libraries.
    links: Vec<Link>,
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
    pub fn linkdirs(&self) -> &[String] {
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

        let directories = parse_field::<Vec<String>>(&map, "linkdirs")?;
        let links = parse_field::<Vec<Link>>(&map, "links")?;

        let use_cxx = parse_field::<bool>(&map, "cxx_used")?;
        let use_stl = parse_field::<bool>(&map, "stl_used")?;

        Ok(BuildInfo {
            linkdirs: directories,
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
            cpp_link_stdlib: None,
            cache: ConfigCache::default(),
        }
    }

    /// Sets the xmake targets for this compilation.
    /// Note that is different from rust target (os and arch), an xmake target
    /// can be binary or a library.
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
    /// This option defaults to `true`.
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
        cmd.arg("lua");

        // In case of xmake is waiting to download something
        cmd.arg("--yes");
        if self.verbose {
            cmd.arg("-v");
        }

        // Get absolute path to the crate root
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let script_file = crate_root.join("src").join("build.lua");
        cmd.arg(script_file);

        if let Some(targets) = &self.targets {
            // :: is used to handle namespaces in xmake but it interferes with the env separator
            // on linux, so we use a different separator
            cmd.env("XMAKERS_TARGETS", targets.replace("::", "||"));
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

            for directory in build_info.linkdirs() {
                // Reference: https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-link-search
                println!("cargo:rustc-link-search=all={}", directory);
            }

            for link in build_info.links() {
                match link.kind() {
                    LinkKind::Static => println!("cargo:rustc-link-lib=static={}", link.name()),
                    LinkKind::Dynamic => println!("cargo:rustc-link-lib=dylib={}", link.name()),
                    LinkKind::Framework if self.cache.plat() == "macosx" => {
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
                    // These runtimes may not be the most appropriate for each platform, but
                    // taken the GNU standard libary is the most common one on linux, and same for
                    // the clang equivalent on windows.
                    // TODO Explore which runtimes is more approriate for macosx
                    let runtime: Option<&str> = match self.cache.plat().as_str() {
                        "linux" => Some("stdc++"),
                        "android" => Some("c++"),
                        _ => None,
                    };

                    if let Some(runtime) = runtime {
                        // Use the kind of crt as a reference
                        let kind = match self.get_static_crt() {
                            true => "static",
                            false => "dylib",
                        };
                        println!(r"cargo:rustc-link-lib={}={}", kind, runtime);
                    }
                }
            }
        }
    }

    // Run the configuration with all the configured
    /// options.
    fn config(&mut self) {
        self.check_version();

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
            cmd.arg(format!("--arch={}", arch));

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

        if let Some(runtimes) = &self.runtimes {
            cmd.arg(format!("--runtimes={}", runtimes));
        } else if self.no_stl_link {
            // Static CRT
            let static_crt = self.static_crt.unwrap_or_else(|| self.get_static_crt());
            let debug = match self.get_mode() {
                // rusct doesn't support debug version of the CRT
                // "debug" => "d",
                // "releasedbg" => "d",
                _ => "",
            };

            let msvc_runtime = match static_crt {
                true => format!("MT{}", debug),
                false => format!("MD{}", debug),
            };
            cmd.arg(format!("--runtimes={},stdc++_static", msvc_runtime));
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

        run(&mut cmd, "xmake");
        dst
    }

    fn get_build_info(&mut self) -> Option<BuildInfo> {
        let mut cmd = self.xmake_command();
        cmd.arg("lua");
        if self.verbose {
            cmd.arg("-v");
        }

        // Get absolute path to the crate root
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let script_file = crate_root.join("src").join("build_info.lua");
        cmd.arg(script_file);

        if let Some(targets) = &self.targets {
            // :: is used to handle namespaces in xmake but it interferes with the env separator
            // on linux, so we use a different separator
            cmd.env("XMAKERS_TARGETS", targets.replace("::", "||"));
        }

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

    fn check_version(&mut self) {
        let version = Version::from_command(self.xmake_executable().as_str());
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
        // To no have the color output
        cmd.env("XMAKE_THEME", "plain");
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
        let stdout = String::from_utf8(output.stdout).ok();
        fail(&format!(
            "command did not execute successfully, got: {}\nstdout: {}",
            output.status,
            stdout.unwrap_or_default()
        ));
    }

    let output = String::from_utf8(output.stdout).ok();

    // if let Some(s) = output.as_deref() {
    //     println!("cargo:warning={}", s);
    // }

    return output;
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
fn parse_info_pairs<T: AsRef<str>>(s: T) -> HashMap<String, Vec<String>> {
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
    fn parse_field(map: &HashMap<String, Vec<String>>, field: &str) -> Result<T, ParsingError>;
}

// Only implement for types that implement DirectParse
impl<T> ParseField<T> for T
where
    T: FromStr + DirectParse,
{
    fn parse_field(map: &HashMap<String, Vec<String>>, field: &str) -> Result<T, ParsingError> {
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
    fn parse_field(
        map: &HashMap<String, Vec<String>>,
        field: &str,
    ) -> Result<Vec<T>, ParsingError> {
        let values = map.get(field).ok_or(ParsingError::MissingKey)?;
        values
            .iter()
            .map(|s| s.parse::<T>().map_err(|_| ParsingError::ParseError))
            .collect::<Result<Vec<T>, ParsingError>>()
    }
}

fn parse_field<T>(map: &HashMap<String, Vec<String>>, field: &str) -> Result<T, ParsingError>
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

    fn from_command(executable: &str) -> Option<Self> {
        let output = run(
            Command::new(executable)
                .arg("--version")
                .env("XMAKE_THEME", "plain"),
            "xmake",
        )?;
        Self::parse(output.as_str())
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse_field, parse_info_pairs, BuildInfo, Link, LinkKind, ParsingError};

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
        let build_info: Result<String, _> = parse_field(&map, "key");
        assert!(map.contains_key("key"));
        assert!(build_info.is_err());
        assert_eq!(build_info.err().unwrap(), ParsingError::MultipleValues);
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
        assert_eq!(build_info.linkdirs(), &expected_directories);
        assert_eq!(build_info.use_cxx(), expected_cxx);
        assert_eq!(build_info.use_stl(), expected_stl);
    }
}
