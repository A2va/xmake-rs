extern crate libc;

extern {
    fn mode() -> libc::c_int;
}

fn main() {
    let output = unsafe { mode() };
    assert!(output == 2);
}
