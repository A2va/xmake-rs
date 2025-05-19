extern crate libc;

extern "C" {
    fn foo() -> libc::c_int;
}

fn main() {
    let output = unsafe { foo() };
    assert!(output == 123, "123 != {}", output);
}
