extern crate xmake;

use std::env;
fn main() {
    // Force release mode
    env::set_var("PROFILE", "release");
    env::set_var("OPT_LEVEL", "3");
    env::set_var("DEBUG", "false");
    xmake::build(".");
}