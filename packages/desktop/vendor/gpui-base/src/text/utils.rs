use std::sync::Arc;

use data_url::DataUrl;
use gpui::{Image, ImageFormat};

/// Decodes a `data:` URL whose mime type names an image format GPUI can
/// decode. Anything else (another scheme, a non-image body, malformed
/// base64) yields `None`, and the caller keeps the URL URI-backed so GPUI's
/// own loader reports the failure.
pub(super) fn data_url_image(url: &str) -> Option<Arc<Image>> {
    let data_url = DataUrl::process(url).ok()?;
    let mime = data_url.mime_type();
    let format = ImageFormat::from_mime_type(&format!("{}/{}", mime.type_, mime.subtype))?;
    let (bytes, _fragment) = data_url.decode_to_vec().ok()?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

#[cfg(test)]
mod tests {
    use gpui::ImageFormat;

    use crate::text::utils::data_url_image;

    #[test]
    fn test_data_url_image() {
        fn image(url: &str) -> (ImageFormat, Vec<u8>) {
            let image = data_url_image(url)
                .unwrap_or_else(|| panic!("expected an embedded image for {url:?}"));
            (image.format(), image.bytes().to_vec())
        }

        assert_eq!(
            image("data:image/png;base64,iVBORw0KGgo="),
            (ImageFormat::Png, b"\x89PNG\r\n\x1a\n".to_vec())
        );
        // Legacy mime aliases and the percent-encoded (non-base64) body form.
        assert_eq!(
            image("data:image/jpg;base64,/9j/4A=="),
            (ImageFormat::Jpeg, b"\xff\xd8\xff\xe0".to_vec())
        );
        assert_eq!(
            image("data:image/svg+xml,%3Csvg%3E%3C/svg%3E"),
            (ImageFormat::Svg, b"<svg></svg>".to_vec())
        );

        assert!(data_url_image("https://example.com/logo.png").is_none());
        assert!(data_url_image("data:text/plain;base64,aGVsbG8=").is_none());
        assert!(data_url_image("data:image/png;base64,not*base64").is_none());
        assert!(data_url_image("data:image/png").is_none());
    }
}
