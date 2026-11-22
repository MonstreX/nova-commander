//! A plugin's message as FAR's `MessageImpl` lays it out: separators with titles, a last one standing for
//! the one above the buttons, a long line read-only or cut, the error wrapped under a separator, the width
//! by the longest line, the title or the buttons within the screen, the first button focused.

use super::*;

fn request(lines: &[&str], buttons: &[&str]) -> MessageRequest {
    MessageRequest {
        title: String::new(),
        lines: lines.iter().map(ToString::to_string).collect(),
        buttons: buttons.iter().map(ToString::to_string).collect(),
        warning: false,
        left_align: false,
        error: None,
    }
}

/// Each item as its kind, its place and its text.
fn shown(form: &Form) -> Vec<(u16, u16, String)> {
    (form.items.iter())
        .map(|item| {
            let text = match &item.control {
                Control::Label(text) => text.clone(),
                Control::Separator => "--".to_string(),
                Control::TitledSeparator { double, title, .. } => {
                    format!("{}{title}", if *double { "==" } else { "-" })
                }
                Control::Edit(input) => format!("[{}]", input.text()),
                Control::Button { label, .. } => format!("<{label}>"),
                other => format!("{other:?}"),
            };
            (item.x, item.y, text)
        })
        .collect()
}

#[test]
fn separators_carry_their_titles_and_a_last_one_is_the_buttons() {
    let message = request(&["one", "\u{1}Part", "two", "\u{2}"], &["OK", "Cancel"]);
    let form = message_form("Title", &message, 80, 20);
    // The buttons' row "[ OK ] [ Cancel ]" is the widest: 17.
    assert_eq!(form.width, 27);
    assert_eq!(
        shown(&form),
        [
            (12, 2, "one".to_string()),
            (0, 3, "-Part".to_string()),
            (12, 4, "two".to_string()),
            (0, 5, "==".to_string()),
            (0, 6, "<OK>".to_string()),
            (0, 6, "<Cancel>".to_string()),
        ]
    );
    assert_eq!(form.height, 9, "no separator of its own above the buttons");
    assert_eq!(form.focus, 4, "the first button");
    let plain = message_form("Title", &request(&["one"], &["OK"]), 80, 20);
    assert_eq!(shown(&plain)[1], (0, 3, "--".to_string()));
    assert_eq!(plain.height, 7);
}

#[test]
fn a_line_longer_than_the_screen_allows_is_read_only_or_cut_without_buttons() {
    let long = "x".repeat(100);
    let form = message_form("", &request(&[&long, "short"], &["OK"]), 60, 20);
    // The screen less ten columns: 49 for the text, the box 59 wide.
    assert_eq!(form.width, 59);
    let item = &form.items[0];
    assert_eq!((item.x, item.y, item.width), (5, 2, 49));
    assert!(item.read_only);
    assert!(matches!(&item.control, Control::Edit(input) if input.text() == long));
    let busy = message_form("", &request(&[&long], &[]), 60, 20);
    assert_eq!(shown(&busy)[0], (5, 2, format!("{}…", "x".repeat(48))));
}

#[test]
fn the_error_comes_wrapped_under_a_separator_and_the_title_widens_the_box() {
    let mut message = request(&["Cannot read"], &["OK"]);
    message.error = Some("0x00000005 - Access is denied. And more words here.".to_string());
    // Wrapped at the screen less eleven columns (FAR's `MAX_MESSAGE_WIDTH`): 29.
    let form = message_form("Error", &message, 40, 20);
    assert_eq!(form.width, 38);
    assert_eq!(
        shown(&form)[..4],
        [
            (13, 2, "Cannot read".to_string()),
            (0, 3, "-".to_string()),
            (8, 4, "0x00000005 - Access is".to_string()),
            (5, 5, "denied. And more words here.".to_string()),
        ]
    );
    message.lines.clear();
    let alone = message_form("Error", &message, 40, 20);
    assert_eq!(
        shown(&alone)[0].2,
        "0x00000005 - Access is",
        "no separator first"
    );
    let titled = message_form("A rather long title", &request(&["a"], &["OK"]), 80, 20);
    assert_eq!(titled.width, 31, "the title with a space each side");
}

#[test]
fn a_left_aligned_messages_separator_titles_its_line_from_the_text_column() {
    let mut message = request(&["a", "\u{1}Part"], &["OK"]);
    message.left_align = true;
    let form = message_form("", &message, 80, 20);
    let item = &form.items[1];
    assert_eq!(item.x, 5, "FAR's X1 of 5");
    assert!(matches!(
        item.control,
        Control::TitledSeparator { left: true, .. }
    ));
    message.buttons.clear();
    let busy = message_form("", &message, 80, 20);
    assert!(
        matches!(
            busy.items[1].control,
            Control::TitledSeparator { left: false, .. }
        ),
        "without buttons FAR centres it"
    );
}

#[test]
fn left_aligned_lines_start_at_the_boxs_text_and_only_rows_that_fit_are_kept() {
    let mut message = request(&["a", "bb", "ccc"], &["OK"]);
    message.left_align = true;
    let form = message_form("", &message, 80, 2);
    assert_eq!(
        shown(&form)[..2],
        [(5, 2, "a".to_string()), (5, 3, "bb".to_string())]
    );
    assert_eq!(shown(&form)[2], (0, 4, "--".to_string()));
}

#[test]
fn text_wraps_between_words_and_a_long_word_where_the_line_ends() {
    assert_eq!(wrapped("one two three", 7), ["one two", "three"]);
    assert_eq!(wrapped("abcdefghij k", 4), ["abcd", "efgh", "ij k"]);
    assert!(wrapped("", 5).is_empty());
}

/// The probe's "Error" through the program: the system's text of the file it could not open, its
/// separators, its long line read-only, the program's Retry / Cancel.
#[cfg(windows)]
#[test]
fn the_probes_error_message_comes_as_fars() {
    use crate::app::dialog::Dialog;
    use crate::app::{App, Overlay};
    let folder = crate::plugin_host::tests::probe_with_markers(&["windows"]);
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    app.set_plugins(crate::plugin_host::tests::load(folder.path(), &|_| true));
    app.run_command_for_test(0, 9, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let form = loop {
        app.pump_plugins();
        if let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay {
            break form.clone();
        }
        assert!(std::time::Instant::now() < deadline, "no message");
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!((form.title.as_str(), form.warning), ("Trouble", true));
    let items = shown(&form);
    assert_eq!(items[1].2, "-Details");
    assert!(items[2].2.starts_with("[Long xxx"), "{:?}", items[2]);
    assert!(form.items[2].read_only);
    assert_eq!(items[3].2, "==Why");
    assert_eq!(items[4].2, "-", "the error's own separator");
    assert!(items[5].2.starts_with("0x00000002 - "), "{:?}", items[5]);
    let buttons: Vec<&str> = (items.iter())
        .filter(|(_, _, text)| text.starts_with('<'))
        .map(|(_, _, text)| text.as_str())
        .collect();
    assert_eq!(buttons, ["<Retry>", "<Cancel>"]);
}

#[test]
fn an_input_line_is_fars_get_string_with_or_without_its_buttons() {
    let mut input = InputRequest {
        title: "T".to_string(),
        prompt: "&Name".to_string(),
        text: "start".to_string(),
        ..InputRequest::default()
    };
    let plain = input_form(&input);
    assert_eq!(
        (plain.width, plain.height, plain.focus),
        (76, 6, INPUT_LINE)
    );
    assert_eq!(
        shown(&plain)[0],
        (5, 2, "&Name".to_string()),
        "shown as it is"
    );
    let line = &plain.items[INPUT_LINE];
    assert_eq!((line.x, line.y, line.width), (5, 3, 66));
    assert!(line.closes && !line.password, "Enter on the line closes");
    assert_eq!(plain.items.len(), 2);
    input.buttons = true;
    input.password = true;
    input.no_ampersand = true;
    input.prompt = "p".repeat(70);
    let full = input_form(&input);
    assert_eq!(full.height, 8);
    assert!(
        matches!(&full.items[0].control, Control::Text(text) if text.chars().count() == 66 && text.ends_with('…'))
    );
    let line = &full.items[INPUT_LINE];
    assert!(line.password && !line.closes, "OK takes Enter");
    assert_eq!(
        shown(&full)[2..],
        [
            (0, 4, "--".to_string()),
            (0, 5, "<OK>".to_string()),
            (0, 5, "<Cancel>".to_string())
        ]
    );
    assert!(matches!(
        full.items[INPUT_OK].control,
        Control::Button { default: true, .. }
    ));
}

#[test]
fn an_input_lines_history_takes_its_last_line_unless_the_plugin_says_not() {
    let mut input = InputRequest {
        history: Some("Folders".to_string()),
        ..InputRequest::default()
    };
    let history = |input: &InputRequest| {
        let form = input_form(input);
        (form.items[INPUT_LINE].history.as_deref())
            .map(|history| (history.name.clone(), history.use_last))
    };
    assert_eq!(history(&input), Some(("Folders".to_string(), true)));
    input.no_last_history = true;
    assert_eq!(history(&input), Some(("Folders".to_string(), false)));
    input.password = true;
    assert_eq!(history(&input), None, "FAR's password line has none");
    input.password = false;
    input.history = None;
    assert_eq!(history(&input), None);
}
