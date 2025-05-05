extern crate libc;

extern "C" {
    fn target() -> libc::c_int;
}

fn main() {
    let output = unsafe { target() };
    assert!(output == 789, "789 != {}", output);
}
