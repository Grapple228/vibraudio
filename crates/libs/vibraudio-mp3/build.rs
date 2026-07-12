fn main() {
    cc::Build::new()
        .file("ffi/minimp3.c")
        .opt_level(2)
        .compile("minimp3");
}
