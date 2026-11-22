use super::*;

/// NetBox's buttons in their order; Append only when it can.
#[test]
fn the_overwrite_buttons_are_netboxs() {
    let labels: Vec<&str> = overwrite_buttons(true)
        .iter()
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(labels[..3], ["&Yes", "&No", "&Cancel"]);
    assert_eq!(labels.len(), 8);
    assert_eq!(overwrite_buttons(false).len(), 7);
    assert_eq!(format_size(1_234_567), "1,234,567");
    assert_eq!(format_size(12), "12");
    let side = Side {
        size: 1000,
        modified: None,
    };
    let text = overwrite_text("a.txt", true, side, side);
    assert!(text.starts_with("Overwrite local file 'a.txt'?"));
    assert!(text.contains("New:      1,000 bytes, Unknown"));
}
