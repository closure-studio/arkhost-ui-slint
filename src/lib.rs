mod app;

#[cfg(feature = "android-app")]
#[no_mangle]
#[tokio::main]
async fn android_main(app: slint::android::AndroidApp) {
    println!(
        "\n### ArkHost-UI-Slint [Version: {}] ###\n",
        app::utils::app_metadata::CARGO_PKG_VERSION.unwrap_or("not found")
    );

    app::utils::db::handle_self_delete(true);
    let data_dir = app.internal_data_path().or(app.external_data_path());
    if let Some(data_dir) = data_dir {
        std::env::set_var(app::env::consts::DATA_DIR, &data_dir);
        println!("Data dir: {}", data_dir.display());
    }

    slint::android::init(app).unwrap();

    let _cleanup_guard = app::utils::db::CleanupGuard::new();
    app::run().await.unwrap();
}
