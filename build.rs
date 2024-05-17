extern crate embed_resource;

fn main() {
    #[cfg(target_os = "windows")]
    static_vcruntime::metabuild();
    #[cfg(target_os = "windows")]
    embed_resource::compile("resource/windows/resources.rc", embed_resource::NONE);

    // slint 内部调用在 debug build 中导致主线程 stack overflow
    #[cfg(all(debug_assertions, windows, target_env = "msvc"))]
    println!("cargo:rustc-link-arg=/STACK:0xA00000"); // 10 MiB

    let config = slint_build::CompilerConfiguration::new().with_style("cosmic".into());
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
