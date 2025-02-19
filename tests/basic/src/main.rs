extern crate libc;

extern {
    fn double_input(input: libc::c_int) -> libc::c_int;
}

fn main() {
    let input = 4;
    let output = unsafe { double_input(input) };
    assert!(output == 8, "{} * 2 != 8", input);
}
