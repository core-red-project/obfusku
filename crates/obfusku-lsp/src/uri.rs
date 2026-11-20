//! Turning the client's document URIs into filesystem paths.

use lsp_types::Uri;
use std::path::PathBuf;

/// The filesystem path a `file://` URI names, or `None` for any other scheme
/// (an `untitled:` buffer has no directory to resolve imports against).
///
/// **Why by hand.** `lsp_types::Uri` wraps `fluent_uri` and, unlike the
/// `url::Url` of earlier `lsp-types` releases, has no `to_file_path`. The
/// decoding is small: strip the scheme, percent-decode the rest.
pub(crate) fn to_path(uri: &Uri) -> Option<PathBuf> {
    path_from_file_uri(uri.as_str())
}

fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?;
    // `file:///C:/x` carries a slash before the drive letter that is not part
    // of a Windows path.
    let bytes = decoded.as_bytes();
    let windows_drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    let path = if windows_drive {
        &decoded[1..]
    } else {
        &decoded[..]
    };
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_paths_are_percent_decoded() {
        assert_eq!(
            path_from_file_uri("file:///home/u/my%20proj/main.obk"),
            Some(PathBuf::from("/home/u/my proj/main.obk"))
        );
    }

    #[test]
    fn windows_drive_paths_lose_the_leading_slash() {
        assert_eq!(
            path_from_file_uri("file:///C:/Users/u/main.obk"),
            Some(PathBuf::from("C:/Users/u/main.obk"))
        );
        assert_eq!(
            path_from_file_uri("file:///c%3A/Users/u/main.obk"),
            Some(PathBuf::from("c:/Users/u/main.obk"))
        );
    }

    #[test]
    fn non_file_schemes_have_no_path() {
        assert_eq!(path_from_file_uri("untitled:Untitled-1"), None);
        assert_eq!(path_from_file_uri("https://example.com/a.obk"), None);
    }

    #[test]
    fn invalid_utf8_after_decoding_has_no_path() {
        assert_eq!(path_from_file_uri("file:///a/%FF.obk"), None);
    }

    #[test]
    fn a_parsed_uri_resolves_through_to_path() {
        let uri: Uri = "file:///tmp/a%20b.obk".parse().unwrap();
        assert_eq!(to_path(&uri), Some(PathBuf::from("/tmp/a b.obk")));
    }
}
