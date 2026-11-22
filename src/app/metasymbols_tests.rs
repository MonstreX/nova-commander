use super::*;
use crate::platform::files::paths::root_name;

fn facts() -> Facts {
    Facts {
        active: PanelFacts {
            dir: r"C:\work".to_string(),
            current: "notes.txt".to_string(),
            selected: vec!["a b.txt".to_string(), "c.rs".to_string()],
        },
        passive: PanelFacts {
            dir: r"\\server\share\docs".to_string(),
            current: "report.doc".to_string(),
            selected: vec!["report.doc".to_string()],
        },
        active_is_left: true,
    }
}

fn expand(command: &str) -> String {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok("LIST".to_string()) };
    let expanded = expand_command(command, &facts(), &mut no_list).unwrap();
    assert!(expanded.prompts.is_empty(), "{command}");
    expanded.fill(&[])
}

#[test]
fn names_extensions_and_folders_of_the_current_file() {
    assert_eq!(expand("edit !.!"), "edit notes.txt");
    assert_eq!(expand("!"), "notes");
    assert_eq!(expand("x!`y"), "xtxty");
    assert_eq!(expand("!~ !`~ !-! !+!"), "notes txt notes.txt notes.txt");
    assert_eq!(expand(r"!\!.!"), r"C:\work\notes.txt");
    assert_eq!(expand("!/ !=\\ !=/"), r"C:\work\ C:\work\ C:\work\");
    assert_eq!(expand("!:"), "C:");
    assert_eq!(expand("100!!"), "100!");
    assert_eq!(expand("[!?!]"), "[]", "no descriptions");
}

#[test]
fn the_selected_files_are_quoted_unless_asked_not_to() {
    assert_eq!(expand("7z a x.7z !&"), r#"7z a x.7z "a b.txt" "c.rs""#);
    assert_eq!(expand("!&Q"), r#""a b.txt" "c.rs""#);
    assert_eq!(expand("!&q"), r#""a b.txt" c.rs"#);
    assert_eq!(expand("!&~"), r#""a b.txt" "c.rs""#);
}

#[test]
fn the_prefixes_switch_the_panel_for_what_follows() {
    assert_eq!(expand(r"!#!\!^!.!"), r"\\server\share\docs\notes.txt");
    assert_eq!(expand("!#!: !.!"), r"\\server\share report.doc");
    assert_eq!(expand("![!.! !]!.!"), "notes.txt report.doc");
    let mut facts = facts();
    facts.active_is_left = false;
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command("![!.!", &facts, &mut no_list).unwrap();
    assert_eq!(expanded.fill(&[]), "report.doc");
}

#[test]
fn a_list_file_is_made_with_its_modifiers() {
    let mut asked = Vec::new();
    let mut make = |panel: &PanelFacts, modifiers: &str| -> io::Result<String> {
        asked.push((panel.selected.clone(), modifiers.to_string()));
        Ok(r"C:\temp\list.tmp".to_string())
    };
    let expanded = expand_command("rar a x.rar @!@FQ! !#!$!", &facts(), &mut make).unwrap();
    assert_eq!(
        expanded.fill(&[]),
        r"rar a x.rar @C:\temp\list.tmp C:\temp\list.tmp"
    );
    assert_eq!(asked[0].1, "FQ");
    assert_eq!(asked[1].0, ["report.doc"], "the passive panel's");
}

#[test]
fn questions_are_asked_with_their_titles_and_initial_texts() {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command(
        "grep !?Search for:?! !?In (!.!):?*.*!",
        &facts(),
        &mut no_list,
    )
    .unwrap();
    assert_eq!(
        expanded.prompts,
        [
            Prompt {
                title: "Search for:".to_string(),
                init: String::new(),
                history: None,
            },
            Prompt {
                title: "In notes.txt:".to_string(),
                init: "*.*".to_string(),
                history: None,
            },
        ]
    );
    assert_eq!(
        expanded.fill(&["fn main".to_string(), "*.rs".to_string()]),
        "grep fn main *.rs"
    );
}

#[test]
fn a_label_shows_its_list_files_and_questions_as_they_are() {
    assert_eq!(
        expand_label("Pack !.! (!@!)", &facts()),
        "Pack notes.txt (!@!)"
    );
    assert_eq!(expand_label("Find !?What?!", &facts()), "Find !?What?!");
}

#[test]
fn drives_of_local_and_network_folders() {
    // Unix paths have neither drives nor shares.
    if cfg!(windows) {
        assert_eq!(root_name(r"D:\a\b"), "D:");
        assert_eq!(root_name(r"\\pc\s\x"), r"\\pc\s");
    }
    assert_eq!(root_name("/home"), "");
}

#[test]
fn brackets_stay_where_nothing_in_them_is_replaced_and_a_history_name_goes() {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command(
        "x !?$GrepHist$Name (optional):?! !?Of (!.!):?!",
        &facts(),
        &mut no_list,
    )
    .unwrap();
    assert_eq!(expanded.prompts[0].title, "Name (optional):");
    assert_eq!(expanded.prompts[1].title, "Of notes.txt:");
}

#[test]
fn the_description_comes_before_a_question_and_a_wrong_one_is_its_mark() {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command("echo !?! x !?Name?!", &facts(), &mut no_list).unwrap();
    assert_eq!(expanded.prompts.len(), 1);
    assert_eq!(expanded.prompts[0].title, "Name");
    assert_eq!(expand("echo !?abc"), "echo !?abc");
}

#[test]
fn a_named_question_keeps_its_history_and_the_answers_fill_user_vars() {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command(
        "tar !?$Archive$Name:?! !?Files:?! %archive.log %USERVAR2 %UserVar1",
        &facts(),
        &mut no_list,
    )
    .unwrap();
    assert_eq!(expanded.prompts[0].history.as_deref(), Some("Archive"));
    assert_eq!(expanded.prompts[0].title, "Name:");
    assert_eq!(
        (
            expanded.prompts[0].history_name(0),
            expanded.prompts[1].history_name(1)
        ),
        ("Archive".to_string(), "UserVar1".to_string())
    );
    assert_eq!(
        expanded.fill(&["a".to_string(), "*.rs".to_string()]),
        "tar a *.rs a.log *.rs a"
    );
}

#[test]
fn an_empty_history_name_is_none_and_replaces_no_percent() {
    let mut no_list = |_: &PanelFacts, _: &str| -> io::Result<String> { Ok(String::new()) };
    let expanded = expand_command("echo !?$$Name:?! %TEMP% 50%", &facts(), &mut no_list).unwrap();
    assert_eq!(expanded.prompts[0].history, None);
    assert_eq!(expanded.prompts[0].history_name(0), "UserVar0");
    assert_eq!(expanded.fill(&["x".to_string()]), "echo x %TEMP% 50%");
    let own = expand_command("echo !?$UserVar0$Name:?! %UserVar0", &facts(), &mut no_list).unwrap();
    assert_eq!(
        own.fill(&["x".to_string()]),
        "echo x %UserVar0",
        "FAR's own name: no %name"
    );
}
