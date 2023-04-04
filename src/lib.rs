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
//!                  .define("FOO", "BAR")
//!                  .cflag("-foo")
//!                  .build();
//! println!("cargo:rustc-link-search=native={}", dst.display());
//! println!("cargo:rustc-link-lib=static=foo");
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::{env, vec};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;


/// Builder style configuration for a pending XMake build.
pub struct Config {
    path: PathBuf,
    target: Option<String>,
    verbose: bool,
    defines: Vec<(OsString, OsString)>,
    out_dir: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    env_cache: HashMap<String, Option<OsString>>
}

impl Config {
    /// Creates a new blank set of configuration to build the project specified
    /// at the path `path`.
    pub fn new<P: AsRef<Path>>(path: P) -> Config {
        Config {
            path: env::current_dir().unwrap().join(path),
            target: None,
            verbose: false,
            defines: Vec::new(),
            out_dir: None,
            env: Vec::new(),
            env_cache: HashMap::new()
        }
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

fn getenv_unwrap(v: &str) -> String {
    match env::var(v) {
        Ok(s) => s,
        Err(..) => fail(&format!("environment variable `{}` not defined", v)),
    }
}

fn fail(s: &str) -> ! {
    panic!("\n{}\n\nbuild script failed, must exit now", s)
}