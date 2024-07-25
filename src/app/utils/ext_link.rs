use log::warn;

pub fn open_ext_link(url: &str) {
    if let Err(e) = open::that(url) {
        warn!("cannot open external link: '{}'; err: {}", url, e);
    }
}
