pub fn open_ext_link(url: &str) {
    if let Err(e) = open::that(url) {
        println!(
            "[open_ext_link] cannot open external link: '{}'; err: {}",
            url, e
        );
    }
}
