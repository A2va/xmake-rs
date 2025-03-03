extern crate libc;

extern {
    fn add(a: libc::c_int, b: libc::c_int) -> libc::c_int;
}

fn main() {
    let output = unsafe { add(4, 4) };
    assert!(output == 8, "4 + 4 != {}", output);
}
