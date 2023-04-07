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
//! let dst = Config::new("libfoo").build();
//! println!("cargo:rustc-link-search=native={}", dst.display());
//! println!("cargo:rustc-link-lib=static=foo");
//! ```
#![deny(missing_docs)]


use std::collections::HashMap;
use std::io::ErrorKind;
use std::{env};
use std::ffi::{OsString, OsStr};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builder style configuration for a pending XMake build.
pub struct Config {
    path: PathBuf,
    target: Option<String>,
    verbose: bool,
    out_dir: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    env_cache: HashMap<String, Option<OsString>>
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
            env: Vec::new(),
            env_cache: HashMap::new()
        }
    }

    /// Sets the xmake target for this compilation.
    pub fn target(&mut self, target: &str) -> &mut Config {
        self.target = Some(target.to_string());
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
        self.install().join("lib")
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
        self.getenv_os("XMAKE").unwrap_or_else(|| OsString::from("xmake"))
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