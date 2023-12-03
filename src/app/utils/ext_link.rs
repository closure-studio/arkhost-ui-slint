use webbrowser;

pub fn open_ext_link(url: &str) {
    if let Err(e) = webbrowser::open(url) {
        eprintln!("[open_ext_link] cannot open external link: '{}'; err: {}", url, e);
    }
}