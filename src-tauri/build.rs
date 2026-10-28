fn main() {
    // 把编译目标三元组写进环境变量，运行时用来拼 binaries/frpc-<triple> 的路径
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=TARGET_TRIPLE={target}");
    tauri_build::build()
}
