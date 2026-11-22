use super::*;
use nc_plugin_sdk::ItemKind;

fn copying<'a>(names: &'a [String], upload: bool, temporary: bool) -> Copying<'a> {
    Copying {
        upload,
        moving: false,
        names,
        temporary,
    }
}

/// NetBox's prompts: one name, or how many; moving says "Move".
#[test]
fn the_prompt_names_the_file_or_counts_them() {
    let one = ["/home/user/report.txt".to_string()];
    assert_eq!(
        prompt(&copying(&one, false, false)),
        "Co&py \"report.txt\" to:"
    );
    let two = ["a".to_string(), "b".to_string()];
    let moving = Copying {
        moving: true,
        ..copying(&two, true, false)
    };
    assert_eq!(prompt(&moving), "Mo&ve 2 items to:");
    assert_eq!(title(true), "Move");
}

/// The target line ends with the folder's separator, then the mask; a typed name is the mask.
#[test]
fn the_target_keeps_the_folder_and_gives_the_mask() {
    let params = CopyParams::default();
    assert_eq!(target_text(r"C:\out", false, &params), r"C:\out\");
    assert_eq!(target_text("/home", true, &params), "/home/");
    let done = confirmed(r"C:\out\*.bak", (true, true), false, r"C:\in", &params);
    assert_eq!(done.folder, r"C:\out\");
    assert_eq!(done.params.file_mask, "*.bak");
    assert!(done.params.newer_only && done.params.queue);
    let kept = confirmed("new.txt", (false, false), true, "/home", &params);
    assert_eq!(kept.folder, "/home");
    assert!(!kept.params.newer_only && !kept.params.queue);
}

/// NetBox's items: the settings in three lines, the presets with "Custom", the reuse check box and the
/// settings' button; the viewer's and the editor's dialog without the target and "New and updated".
#[test]
fn the_items_are_netboxs_and_their_slots_match() {
    let one = ["a.txt".to_string()];
    let params = CopyParams::default();
    for temporary in [false, true] {
        let slots = slots(temporary);
        let items = items(&copying(&one, false, temporary), r"C:\out", &params);
        assert_eq!(items.len(), slots.ok + 2, "temporary: {temporary}");
        assert_eq!(items[slots.info].text, "Transfer type: Binary");
        assert_eq!(items[slots.preset].kind, ItemKind::List);
        assert_eq!(
            items[slots.preset].lines,
            [
                "Default",
                "Text files (ASCII)",
                "Binary all",
                "No preserve time",
                "Custom"
            ]
        );
        assert_eq!(items[slots.reuse].text, "Use &same settings next time");
        assert_eq!(items[slots.settings].text, "Transfer settin&gs...");
        assert!(items[slots.settings].no_close && !items[slots.settings].disabled);
        assert_eq!(items[slots.ok].text, "&Ok");
        match (slots.target, slots.newer_only) {
            (Some(target), Some(newer)) => {
                assert!(!temporary);
                assert_eq!(items[target].text, r"C:\out\");
                assert_eq!(items[newer].text, "&New and updated file(s) only");
            }
            (None, None) => assert!(temporary),
            other => panic!("{other:?}"),
        }
    }
}

/// NetBox's `NB_TRANSFER_QUEUE` after "New and updated", checked as the settings say; none for the
/// viewer's and the editor's copies.
#[test]
fn transfer_on_background_follows_new_and_updated() {
    let one = ["a.txt".to_string()];
    let queued = CopyParams {
        queue: true,
        ..CopyParams::default()
    };
    let shown = slots(false);
    let (Some(newer), Some(queue)) = (shown.newer_only, shown.queue) else {
        panic!("both there");
    };
    assert_eq!(queue, newer + 1);
    let items = items(&copying(&one, false, false), r"C:\out", &queued);
    assert_eq!(items[queue].text, "Transfer on background (add to &queue)");
    assert_eq!(items[queue].kind, ItemKind::Check);
    assert_eq!(items[queue].state, 1, "checked as the settings say");
    assert_eq!(slots(true).queue, None);
}

/// The settings' line wrapped into the lister's three rows.
#[test]
fn the_settings_wrap_into_three_lines() {
    let params = CopyParams {
        include_mask: "*.txt; *.log; *.md; *.csv; *.json; *.xml; *.yaml".to_string(),
        preserve_time: false,
        calculate_size: false,
        speed_limit: 1024,
        newer_only: true,
        ..CopyParams::default()
    };
    let lines = info_lines(&params, Attrs::download());
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|line| line.chars().count() <= INFO_WIDTH));
    assert!(lines[0].starts_with("Transfer type: Binary; Do not preserve timestamp;"));
    assert!(!lines[1].is_empty());
    assert_eq!(info_lines(&CopyParams::default(), Attrs::default())[1], "");
}
