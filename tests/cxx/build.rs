extern crate xmake;

fn main() {
    xmake::Config::new(".").verbose(true).build();

    // xmake::build(".");
}