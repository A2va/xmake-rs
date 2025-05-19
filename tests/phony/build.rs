extern crate xmake;

fn main() {
    let mut config = xmake::Config::new(".");
    config.build();

    let foo_includedirs = config.build_info().includedirs(xmake::Source::Package, "xmrs-foo");
    assert_eq!(foo_includedirs.is_empty(), false);

    let f = foo_includedirs.first().unwrap();
    assert!(f.join("foo").join("foo.h").exists());
}