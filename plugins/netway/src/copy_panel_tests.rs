use super::*;

/// A typed server folder: absolute as it is, relative from the panel's, without its trailing `/`.
#[test]
fn a_typed_remote_folder_is_resolved() {
    assert_eq!(remote_folder("/srv/", "/home"), "/srv");
    assert_eq!(remote_folder("docs/", "/home"), "/home/docs");
    assert_eq!(remote_folder("/", "/home"), "/");
}
