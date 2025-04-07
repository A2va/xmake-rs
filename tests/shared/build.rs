extern crate xmake;
use xmake::Config;

fn main() {
    Config::new(".").targets("foo").build();
}