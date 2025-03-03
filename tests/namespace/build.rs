extern crate xmake;
use xmake::Config;

fn main() {
    Config::new(".").targets("ns1::foo").build();
}