use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::StreamVariant;

/// RFC 3986 userinfo: encode everything except the unreserved set.
const USERINFO: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

pub fn rtsp_url(host: &str, port: u16, user: &str, pass: &str, v: StreamVariant) -> String {
    let user = utf8_percent_encode(user, USERINFO);
    let pass = utf8_percent_encode(pass, USERINFO);
    format!("rtsp://{user}:{pass}@{host}:{port}/{}", v.path())
}

/// Replaces the password in an RTSP URL with `***` (safe for logs and UI).
pub fn mask(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let Some((userinfo, tail)) = rest.split_once('@') else { return url.to_string() };
    let user = userinfo.split(':').next().unwrap_or("");
    format!("{scheme}://{user}:***@{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_basic_url() {
        assert_eq!(
            rtsp_url("192.168.0.20", 554, "cam", "pw", StreamVariant::High),
            "rtsp://cam:pw@192.168.0.20:554/stream1"
        );
        assert_eq!(
            rtsp_url("192.168.0.20", 8554, "cam", "pw", StreamVariant::Low),
            "rtsp://cam:pw@192.168.0.20:8554/stream2"
        );
    }

    #[test]
    fn url_encodes_special_chars() {
        let url = rtsp_url("10.0.0.5", 554, "my user", "p@ss:w/rd #1%", StreamVariant::High);
        assert_eq!(url, "rtsp://my%20user:p%40ss%3Aw%2Frd%20%231%25@10.0.0.5:554/stream1");
    }

    #[test]
    fn mask_hides_password() {
        assert_eq!(mask("rtsp://cam:p%40ss@h:554/stream2"), "rtsp://cam:***@h:554/stream2");
    }

    #[test]
    fn mask_leaves_url_without_credentials() {
        assert_eq!(mask("rtsp://h:554/stream2"), "rtsp://h:554/stream2");
    }
}
