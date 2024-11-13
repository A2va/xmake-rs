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
use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builder style configuration for a pending XMake build.
pub struct Config {
    path: PathBuf,
    target: Option<String>,
    verbose: bool,
    out_dir: Option<PathBuf>,
    mode: Option<String>,
    options: Vec<(OsString, OsString)>,
    env: Vec<(OsString, OsString)>,
    static_crt: Option<bool>,
    cpp_link_stdlib: Option<String>,
    env_cache: HashMap<String, Option<OsString>>,
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
            out_dir: None,
            mode: None,
            options: Vec::new(),
            env: Vec::new(),
            static_crt: None,
            cpp_link_stdlib: None,
            env_cache: HashMap::new(),
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
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.options
            .push((key.as_ref().to_owned(), value.as_ref().to_owned()));
        self
    }

    /// Configure an environment variable for the `xmake` processes spawned by
    /// this crate in the `build` step.
    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Config
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
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

        // List of xmake platform https://github.com/xmake-io/xmake/tree/master/xmake/platforms
        let os = getenv_unwrap("CARGO_CFG_TARGET_OS");
        let plat = match self.get_xmake_plat(os.clone()) {
            Some(p) => p,
            None => panic!("unsupported rust target: {}", os),
        };

        if host != target {
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
            let option = format!(
                "--{}={}",
                key.clone().into_string().unwrap(),
                val.clone().into_string().unwrap()
            );
            cmd.arg(option);
        }

        run(&mut cmd, "xmake");
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

    fn xmake_executable(&mut self) -> OsString {
        self.getenv_os("XMAKE")
            .unwrap_or_else(|| OsString::from("xmake"))
    }

    fn getenv_os(&mut self, v: &str) -> Option<OsString> {
        if let Some(val) = self.env_cache.get(v) {
            return val.clone();
        }
        let r = env::var_os(v);
        println!("{} = {:?}", v, r);
        self.env_cache.insert(v.to_string(), r.clone());
        r
    }
}

fn run(cmd: &mut Command, program: &str) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            fail(&format!(
                "failed to execute command: {}\nis `{}` not installed?",
                e, program
            ));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !status.success() {
        fail(&format!(
            "command did not execute successfully, got: {}",
            status
        ));
    }
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
