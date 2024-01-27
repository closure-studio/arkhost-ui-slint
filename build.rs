extern crate embed_resource;

fn main() {
    if cfg!(target_os = "windows") {
        static_vcruntime::metabuild();
        embed_resource::compile("resource/windows/resources.rc", embed_resource::NONE);
    }

    slint_build::compile("ui/app-window.slint").unwrap();
}
