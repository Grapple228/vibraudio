use std::env;
use std::path::PathBuf;

fn main() {
    let mut build = cc::Build::new();
    build.file("ffi/minimp3.c").opt_level(3);

    // Проверяем, собираем ли мы под wasm
    let target = env::var("TARGET").unwrap();
    if target == "wasm32-unknown-unknown" {
        // Путь к WASI SDK (можно переопределить через переменную окружения)
        let wasi_sdk = env::var("WASI_SDK_PATH").unwrap_or_else(|_| "/opt/wasi-sdk".to_string());
        let wasi_sdk_path = PathBuf::from(wasi_sdk);

        let clang = wasi_sdk_path.join("bin").join("clang");
        let ar = wasi_sdk_path.join("bin").join("llvm-ar");
        let sysroot = wasi_sdk_path.join("share").join("wasi-sysroot");

        build.compiler(clang);
        build.archiver(ar);
        build.flag("--target=wasm32-wasi");
        build.flag(&format!("--sysroot={}", sysroot.display()));
    }

    build.compile("minimp3");
}
