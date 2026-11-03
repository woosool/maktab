//! Passive native rendering input: MIME HTML + embedded raster images, never active content.
use crate::{
    html_paste::{Fragment, import_html_with_images},
    rich_text::InlineImage,
};
use mailparse::MailHeaderMap;
use std::{
    collections::HashMap,
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    time::{Duration, Instant},
};
fn cid(value: &str) -> String {
    percent_encoding::percent_decode_str(value.trim().trim_matches(['<', '>', '"']))
        .decode_utf8_lossy()
        .to_lowercase()
}
pub fn from_mime(raw: &[u8]) -> Result<Option<Fragment>, String> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| e.to_string())?;
    let mut html = None;
    let mut images = HashMap::new();
    collect(&parsed, &mut html, &mut images);
    let Some(html) = html else { return Ok(None) };
    let resolve = |src: &str| {
        let key = if src.get(..4).is_some_and(|s| s.eq_ignore_ascii_case("cid:")) {
            cid(&src[4..])
        } else {
            cid(src)
        };
        if let Some(image) = images.get(&key) {
            return Some(image.clone());
        }
        if src.starts_with("https://") || src.starts_with("http://") {
            return Some(InlineImage {
                cid: src.into(),
                name: "Remote image".into(),
                mime: String::new(),
                data: Vec::new(),
                position: 0,
                width: 240,
                alt: "Remote image".into(),
            });
        }
        None
    };
    import_html_with_images(&html, &resolve).map(Some)
}
fn collect(
    part: &mailparse::ParsedMail<'_>,
    html: &mut Option<String>,
    images: &mut HashMap<String, InlineImage>,
) {
    let disposition = part.get_content_disposition();
    let attached = disposition.disposition == mailparse::DispositionType::Attachment;
    if part.ctype.mimetype == "message/rfc822"
        || (attached && part.ctype.mimetype.starts_with("multipart/"))
    {
        return;
    }
    if part.ctype.mimetype == "text/html"
        && !attached
        && !disposition.params.contains_key("filename")
        && html.is_none()
    {
        *html = part.get_body().ok();
    }
    if (part.ctype.mimetype.starts_with("image/")
        || part.headers.get_first_value("Content-ID").is_some())
        && let Ok(data) = part.get_body_raw()
        && let Ok((width, mime)) = validate_image(&data)
    {
        let image = InlineImage {
            cid: part
                .headers
                .get_first_value("Content-ID")
                .map(|s| cid(&s))
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: disposition
                .params
                .get("filename")
                .or_else(|| part.ctype.params.get("name"))
                .cloned()
                .unwrap_or_else(|| "Embedded image".into()),
            mime,
            data,
            position: 0,
            width: width.min(760),
            alt: "Embedded image".into(),
        };
        if let Some(id) = part.headers.get_first_value("Content-ID") {
            images.insert(cid(&id), image.clone());
        }
        if let Some(location) = part.headers.get_first_value("Content-Location") {
            images.insert(cid(&location), image);
        }
    }

    for child in &part.subparts {
        collect(child, html, images);
    }
}
pub fn validate_image(data: &[u8]) -> Result<(u32, String), String> {
    if data.len() > 10 * 1024 * 1024 {
        return Err("Image exceeds the 10 MB limit".into());
    }
    let format = image::guess_format(data).map_err(|_| "Unsupported image format")?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
    ) {
        return Err("Unsupported image format".into());
    }
    let (w, h) = image::ImageReader::with_format(std::io::Cursor::new(data), format)
        .into_dimensions()
        .map_err(|_| "Invalid image")?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 || u64::from(w) * u64::from(h) > 32_000_000 {
        return Err("Image dimensions exceed the rendering limit".into());
    }
    Ok((w, format.to_mime_type().into()))
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && !ip.is_documentation()
                && ip.octets()[0] != 0
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                public_ip(IpAddr::V4(v4))
            } else {
                !ip.is_loopback()
                    && !ip.is_unspecified()
                    && !ip.is_multicast()
                    && !ip.is_unique_local()
                    && !ip.is_unicast_link_local()
                    && (ip.segments()[0] & 0xffc0) != 0xfec0
            }
        }
    }
}
fn fetch(src: &str) -> Result<Vec<u8>, String> {
    let url = url::Url::parse(src).map_err(|_| "Invalid image URL")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("Only HTTPS images without embedded credentials can be loaded".into());
    }
    let host = url.host_str().ok_or("Image URL has no host")?;
    let addresses: Vec<_> = (host, url.port_or_known_default().unwrap_or(443))
        .to_socket_addrs()
        .map_err(|_| "Image host could not be resolved")?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return Err("Images from local or private network addresses are blocked".into());
    }
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| "Could not initialize image loading")?;
    let response = http
        .get(url)
        .header("Accept", "image/png,image/jpeg,image/gif,image/webp")
        .send()
        .map_err(|_| "Image could not be downloaded")?;
    if !response.status().is_success() {
        return Err("Image server returned an error or redirect".into());
    }
    if response
        .content_length()
        .is_some_and(|n| n > 10 * 1024 * 1024)
    {
        return Err("Image exceeds the 10 MB limit".into());
    }
    let mut data = Vec::new();
    response
        .take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "Image download was interrupted")?;
    validate_image(&data)?;
    Ok(data)
}
pub type ImageResult = (String, Result<Vec<u8>, String>);
pub fn load_remote(urls: Vec<String>) -> Vec<ImageResult> {
    let started = Instant::now();
    let mut total = 0;
    urls.into_iter()
        .take(32)
        .map(|url| {
            let result = if started.elapsed() > Duration::from_secs(60) || total > 20 * 1024 * 1024
            {
                Err("Message image download limit reached".into())
            } else {
                fetch(&url)
            };
            if let Ok(data) = &result {
                total += data.len();
            }
            (url, result)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternative_html_resolves_related_signature_images_and_keeps_formatting() {
        use base64::Engine;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(3, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes.into_inner());
        let raw = format!(
            "MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=r\r\n\r\n--r\r\nContent-Type: multipart/alternative; boundary=a\r\n\r\n--a\r\nContent-Type: text/plain\r\n\r\nPlain fallback\r\n--a\r\nContent-Type: text/html\r\n\r\n<h1>Hello</h1><p><b>Signature</b><img src='cid:logo%40example' style='width:120px' alt='Company logo'></p><script>bad()</script><img src='https://images.example/signature.png'>\r\n--a--\r\n--r\r\nContent-Type: image/png\r\nContent-ID: <logo@example>\r\nContent-Disposition: inline; filename=logo.png\r\nContent-Transfer-Encoding: base64\r\n\r\n{encoded}\r\n--r--\r\n"
        );
        let fragment = from_mime(raw.as_bytes()).unwrap().unwrap();
        assert!(fragment.text.contains("Hello"));
        assert!(!fragment.text.contains("Plain fallback"));
        assert!(!fragment.text.contains("bad()"));
        assert_eq!(fragment.rich.images.len(), 2);
        assert_eq!(fragment.rich.images[0].width, 120);
        assert!(!fragment.rich.images[0].data.is_empty());
        assert!(fragment.rich.images[1].data.is_empty());
        assert!(
            fragment
                .rich
                .spans
                .iter()
                .any(|s| s.mark == crate::rich_text::Mark::Bold)
        );
    }
    #[test]
    fn plain_messages_and_html_attachments_do_not_replace_the_body() {
        assert!(
            from_mime(b"Content-Type: text/plain\r\n\r\nHello")
                .unwrap()
                .is_none()
        );
        assert!(from_mime(b"Content-Type: text/html\r\nContent-Disposition: attachment; filename=attached.html\r\n\r\n<h1>Attachment</h1>").unwrap().is_none());
    }
    #[test]
    fn remote_images_never_contact_private_networks() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "::1",
            "::ffff:127.0.0.1",
            "fe80::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()));
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(fetch("http://example.com/image.png").is_err());
    }
}
