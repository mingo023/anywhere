use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use data_url::DataUrl;
use gpui::{Image, ImageFormat};
use url::Url;

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

/// The file a relative or `file:` URL names, with relative ones resolved
/// against `base`, the document's directory. Any other scheme yields `None`.
pub(super) fn local_image_path(url: &str, base: &Path) -> Option<PathBuf> {
    let url = Url::from_directory_path(base).ok()?.join(url).ok()?;
    (url.scheme() == "file").then(|| url.to_file_path().ok())?
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use gpui::ImageFormat;

    use crate::text::utils::{data_url_image, local_image_path};

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

    #[test]
    fn test_local_image_path() {
        fn path(url: &str) -> Option<PathBuf> {
            local_image_path(url, Path::new("/repo/docs"))
        }

        assert_eq!(path("a.png"), Some("/repo/docs/a.png".into()));
        assert_eq!(path("./img/a.png"), Some("/repo/docs/img/a.png".into()));
        assert_eq!(path("../a.png"), Some("/repo/a.png".into()));
        assert_eq!(path("/abs/a.png"), Some("/abs/a.png".into()));
        assert_eq!(path("file:///abs/a.png"), Some("/abs/a.png".into()));
        assert_eq!(path("my%20shot.png"), Some("/repo/docs/my shot.png".into()));
        assert_eq!(path("my shot.png"), Some("/repo/docs/my shot.png".into()));
        assert_eq!(path("a.png?raw=true#top"), Some("/repo/docs/a.png".into()));

        assert_eq!(path("https://example.com/a.png"), None);
        assert_eq!(path("data:image/png;base64,iVBORw0KGgo="), None);
    }
}
