extern crate xmake;

fn main() {
    // Builds the project in the directory located in `libdouble`, installing it
    // into $OUT_DIR
    let dst = xmake::build("libdouble");

    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-lib=static=libdouble");
}

