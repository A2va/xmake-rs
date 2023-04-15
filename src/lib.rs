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
//! xmake = "0.1"
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
            env_cache: HashMap::new(),
            static_crt: None,
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

    /// Configure an environment variable for the `xmake` processes spawned by
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

    /// Run this configuration, compiling the library with all the configured
    /// options.
    ///
    /// This will run both the configuration command as well as the
    /// command to build the library.
    pub fn build(&mut self) -> PathBuf {
        self.config();

        let mut cmd = self.xmake_command();
        cmd.arg("build");
        if self.target.is_some() {
            cmd.arg(self.target.clone().unwrap());
        }

        cmd.arg("-F").arg(self.path.clone().join("xmake.lua"));

        // In case of xmake is waiting to download something
        cmd.arg("--yes");
        if self.verbose {
            cmd.arg("-v");
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
        cmd.arg("-F").arg(self.path.clone().join("xmake.lua"));

        let dst = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(getenv_unwrap("OUT_DIR")));

        cmd.arg("-o").arg(dst.join("build"));

        if let Some(static_crt) = self.static_crt {
            match static_crt {
                true => cmd.arg("--vs_runtime=MT"),
                false => cmd.arg("--vs_runtime=MD"),
            };
        }

        let mode = self.get_mode();
        cmd.arg("-m").arg(mode);

        for &(ref k, ref v) in self.options.iter().chain(&self.options) {
            let option = format!(
                "--{}={}",
                k.clone().into_string().unwrap(),
                v.clone().into_string().unwrap()
            );
            cmd.arg(option);
        }

        if self.verbose {
            cmd.arg("-v");
        }
        run(&mut cmd, "xmake");
    }

    /// Install target in OUT_DIR.
    fn install(&mut self) -> PathBuf {
        let mut cmd = self.xmake_command();
        cmd.arg("install");
        if self.target.is_some() {
            cmd.arg(self.target.clone().unwrap());
        }
        cmd.arg("-F").arg(self.path.clone().join("xmake.lua"));

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
