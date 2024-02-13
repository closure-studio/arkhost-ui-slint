extern crate embed_resource;

fn main() {
    #[cfg(target_os = "windows")]
    static_vcruntime::metabuild();
    #[cfg(target_os = "windows")]
    embed_resource::compile("resource/windows/resources.rc", embed_resource::NONE);

    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into());
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
