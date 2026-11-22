//! FAR's copy, move and link dialogs: their lines, targets and items.

use super::*;

#[test]
fn several_destinations_split_outside_quotes() {
    assert_eq!(
        split_targets(r#"D:\a; "E:\b,c" ,F:\d;"#),
        ["D:\\a", "E:\\b,c", "F:\\d"]
    );
    assert_eq!(unquote(r#""E:\b;c""#), "E:\\b;c");
}

#[test]
fn a_name_that_ends_with_a_space_keeps_it_through_the_line() {
    let sources = [VfsPath::local("C:\\work\\a ")];
    for move_ in [false, true] {
        let line = initial_target(&sources, "C:\\work", "C:\\work", move_);
        assert_eq!(line, "\"a \"");
        assert_eq!(unquote(&line), "a ");
        assert_eq!(split_targets(&line), ["a "]);
    }
    assert_eq!(unquote("  a  "), "a");
    assert_eq!(
        unquote("\"abc"),
        "abc",
        "a stray quote is no part of a name"
    );
    assert_eq!(split_targets(" a ; \" b \" ,, c d "), ["a", " b ", "c d"]);
}

#[test]
fn the_target_is_the_other_folder_or_the_name_itself_and_quoted_when_it_has_separators() {
    let sources = [VfsPath::local("C:\\work\\a.txt")];
    let here = "C:\\work";
    assert_eq!(initial_target(&sources, here, "D:\\x", false), "D:\\x\\");
    assert_eq!(initial_target(&sources, here, "C:\\WORK", false), "a.txt");
    assert_eq!(
        initial_target(&sources, here, "probe://one/docs", false),
        "probe://one/docs/",
        "a plugin's panel: its own separator"
    );
    assert_eq!(
        initial_target(&sources, here, "probe://one/", false),
        "probe://one/"
    );
    assert_eq!(
        initial_target(&sources, here, "D:\\a;b", false),
        "\"D:\\a;b\\\""
    );
    assert_eq!(initial_target(&sources, here, "D:\\a;b", true), "D:\\a;b\\");
}

#[test]
fn the_dialogs_answers_say_what_to_keep() {
    let mut form = transfer_form("", "D:\\", false);
    for item in &mut form.items {
        if let Control::Radio { checked, .. } = &mut item.control {
            *checked = false;
        }
    }
    let check = |form: &mut Form, index: usize| match &mut form.items[index].control {
        Control::Radio { checked, .. } | Control::Check { checked, .. } => *checked = true,
        _ => panic!("not a box"),
    };
    check(&mut form, SECURITY_INHERIT);
    check(&mut form, PRESERVE_TIMESTAMPS);
    check(&mut form, LINK_CONTENTS);
    check(&mut form, VERIFY);

    let keep = TransferAnswers::from_form(&form, Operation::Copy).keep();

    assert_eq!(keep.rights, Rights::Inherit);
    assert!(keep.timestamps && keep.link_contents && keep.verify);
}

#[test]
fn the_dialog_has_fars_items_and_the_target_is_selected() {
    let form = transfer_form("Copy \"a\" t&o:", "D:\\", false);
    assert_eq!(form.focus, TARGET);
    let Some(Control::Edit(input)) = form.control(TARGET) else {
        panic!("the target line");
    };
    assert_eq!(input.selected_text().as_deref(), Some("D:\\"));
    assert!(matches!(
        form.control(COPY_BUTTON),
        Some(Control::Button { default: true, .. })
    ));
    let answers = TransferAnswers::from_form(&form, Operation::Copy);
    assert_eq!(
        answers,
        TransferAnswers {
            targets: vec!["D:\\".to_string()],
            existing: 0,
            ask_read_only: true,
            security: 0,
            preserve_timestamps: false,
            link_contents: false,
            verify: false,
            link: None,
            use_filter: false,
        }
    );
    assert_eq!(answers.keep(), Keep::default());
    assert!(matches!(
        form.control(VERIFY),
        Some(Control::Check { label, checked: false }) if label == "&Verify the copies"
    ));
    let moving = transfer_form("", "D:\\", true);
    assert!(
        moving.items[MULTIPLE_TARGETS].disabled,
        "several destinations: copy only"
    );
    assert_eq!(moving.title, "Rename/Move");
}

#[test]
fn the_link_dialog_has_fars_items() {
    let sources = [VfsPath::local("C:\\work\\a.txt")];
    let form = link_form(&prompt(&sources, Operation::Link, false), "D:\\x\\", true);
    assert_eq!(form.title, "Link");
    assert_eq!(form.height, LINK_HEIGHT);
    assert!(matches!(
        form.control(0),
        Some(Control::Text(text)) if text == "Create link to \"a.txt\" &in:"
    ));
    assert!(matches!(
        form.control(LINK_BUTTON),
        Some(Control::Button { label, default: true }) if label == "Create &link"
    ));
    assert!(matches!(
        form.control(LINK_MULTIPLE_TARGETS),
        Some(Control::Check { label, .. }) if label == "Process &multiple destinations"
    ));
}

#[test]
fn the_link_type_starts_on_hard_links_for_files_and_on_junctions_for_folders() {
    let sources = [VfsPath::local("C:\\work\\a.txt")];
    let form = link_form("", "D:\\x\\", true);
    let answers = TransferAnswers::from_form(&form, Operation::Link);
    assert_eq!(answers.link, Some(NewLink::Hard));
    assert_eq!(answers.targets, ["D:\\x\\"]);
    assert_eq!(answers.keep(), Keep::default());

    let folders = link_form("", "D:\\x\\", false);
    let answers = TransferAnswers::from_form(&folders, Operation::Link);
    assert_eq!(answers.link, Some(NewLink::Junction), "folders alone");
    assert_eq!(
        prompt(
            &[sources[0].clone(), sources[0].clone()],
            Operation::Link,
            false
        ),
        "Create links to 2 items &in:"
    );
}

#[test]
fn a_tree_s_long_path_is_cut_where_far_s_line_ends() {
    let long = format!(r"R:\{}", "folder\\".repeat(20));
    let line = prompt(&[VfsPath::local(&long)], Operation::Copy, true);
    // FAR: 67 columns inside the box, the name 56 of them (`&` draws nothing).
    assert_eq!(line.chars().count() - 1, 67, "{line}");
    assert!(line.starts_with(r#"Copy "R:\folder\"#) && line.ends_with("\u{2026}\" t&o:"));
    let short = prompt(&[VfsPath::local(r"R:\a")], Operation::Copy, true);
    assert_eq!(short, r#"Copy "R:\a" t&o:"#);
}
