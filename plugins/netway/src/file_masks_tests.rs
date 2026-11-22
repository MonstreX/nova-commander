use super::*;

fn masks(text: &str) -> FileMasks {
    FileMasks::parse(text).expect("masks read")
}

fn file(text: &str, path: &str) -> bool {
    masks(text).matches(path, false, false, None)
}

fn folder(text: &str, path: &str) -> bool {
    masks(text).matches(path, false, true, None)
}

#[test]
fn names_match_without_regard_to_case_with_wildcards_and_sets() {
    assert!(glob("*.TXT", "notes.txt"));
    assert!(glob("a?c", "abc") && !glob("a?c", "ac"));
    assert!(glob("[a-c]*", "beta") && !glob("[a-c]*", "delta"));
    assert!(glob("[!a-c]*", "delta") && !glob("[!a-c]*", "alpha"));
    assert!(glob("*", "") && glob("x*y*z", "xAAyBBz"));
}

#[test]
fn masks_split_by_semicolons_and_commas_exclude_after_the_bar() {
    let typed = "*.txt; *.log, *.md | secret*";
    assert!(file(typed, "/home/a.txt") && file(typed, "/home/b.log") && file(typed, "/c.md"));
    assert!(!file(typed, "/home/a.bin"), "not included");
    assert!(!file(typed, "/home/secret.txt"), "excluded");
    assert!(
        file("| *.bak", "/a.txt") && !file("| *.bak", "/a.bak"),
        "only excluding"
    );
    assert!(file("", "/anything"), "no masks: all");
    assert!(
        file("a;;b.txt", "/a;b.txt"),
        "a doubled delimiter is the character"
    );
}

#[test]
fn star_dot_takes_names_without_an_extension_and_any_masks_take_all() {
    assert!(file("*.", "/home/README") && !file("*.", "/home/a.txt"));
    assert!(file("*.*", "/home/README") && file("*", "/a.b"));
}

#[test]
fn a_mask_ending_with_a_slash_is_a_folders_and_leaves_files_alone() {
    let typed = "| .svn/; *.tmp";
    assert!(!folder(typed, "/src/.svn"), "the folder excluded");
    assert!(folder(typed, "/src/lib"));
    assert!(file(typed, "/src/.svn"), "a file of that name is no folder");
    assert!(!file(typed, "/src/a.tmp"));
    // Folders only included: other folders left out, files in them all taken.
    assert!(folder("docs/", "/home/docs") && !folder("docs/", "/home/src"));
    assert!(
        folder("docs/", "/home/docs/deep"),
        "a folder inside an included one"
    );
    assert!(file("docs/", "/home/src/a.txt"), "no file masks: files all");
}

#[test]
fn a_path_before_the_name_matches_the_items_folder() {
    assert!(file("/home/*/*.txt", "/home/user/a.txt"));
    assert!(!file("/home/*/*.txt", "/srv/user/a.txt"));
    assert!(
        file("/home/user/", "/home/user/a.txt"),
        "a folder's mask: files all"
    );
    assert!(folder("/home/user/", "/home/user"));
    assert!(!folder("/home/user/", "/home/other"));
    let local = masks("C:\\Data\\*.txt");
    assert!(local.matches("C:\\Data\\a.txt", true, false, None));
    assert!(!local.matches("D:\\Data\\a.txt", true, false, None));
}

#[test]
fn sizes_bound_files_with_units() {
    let sized = masks("*>1K");
    let with = |size| Some(Params { size, modified: 0 });
    assert!(sized.matches("/a", false, false, with(1025)));
    assert!(
        !sized.matches("/a", false, false, with(1024)),
        "above, not at"
    );
    assert!(masks("*>=1K").matches("/a", false, false, with(1024)));
    assert!(masks("*<2M").matches("/a", false, false, with(2 * 1024 * 1024 - 1)));
    assert!(!masks("*<=1G").matches("/a", false, false, with((1 << 30) + 1)));
    assert!(
        !sized.matches("/a", false, false, None),
        "no size known: not taken"
    );
    assert_eq!(size_of("10K"), Some(10 * 1024));
    assert_eq!(size_of("3 m"), Some(3 << 20));
    assert_eq!(size_of("5X"), None);
}

#[test]
fn times_bound_files_by_date_or_by_a_span_back() {
    let noon = standard_time("2024-03-10 12:00").expect("a date");
    assert_eq!(standard_time("2024-03-10 12:00:30"), Some(noon + 30));
    assert_eq!(standard_time("2024-03-10"), Some(noon - 12 * 3600));
    assert_eq!(standard_time("2024-03-10 12"), None);
    let after = masks("*>2024-03-10 12:00");
    let at = |modified| Some(Params { size: 0, modified });
    assert!(after.matches("/a", false, false, at(noon + 1)));
    assert!(!after.matches("/a", false, false, at(noon)));
    let now = UNIX_TICKS + 1_700_000_000 * TICKS_PER_SECOND;
    assert_eq!(relative_time("1D", now), Some(1_700_000_000 - 86_400));
    assert_eq!(relative_time("30n", now), Some(1_700_000_000 - 1_800));
    assert_eq!(relative_time("2H", now), Some(1_700_000_000 - 7_200));
    let today = relative_time("today", now).expect("today's start");
    assert!(today <= 1_700_000_000 && 1_700_000_000 - today < 86_400);
    assert_eq!(relative_time("yesterday", now), relative_time("1DS", now));
    assert_eq!(relative_time("1X", now), None);
    assert!(relative_time("1Y", now).is_some_and(|year| 1_700_000_000 - year > 364 * 86_400));
}

#[test]
fn wrong_masks_say_where() {
    assert_eq!(FileMasks::parse("*.txt>abc"), Err(mask_error("*.txt>abc")));
    assert_eq!(FileMasks::parse("a | b | c"), Err(mask_error("a | b | c")));
    assert_eq!(
        FileMasks::parse("*>1K>2K"),
        Err(mask_error("*>1K>2K")),
        "two lower bounds"
    );
    assert_eq!(
        FileMasks::parse("dir/>1K"),
        Err(mask_error("dir/>1K")),
        "a folder's size"
    );
    assert_eq!(masks(" *.txt ").text(), " *.txt ");
    assert!(masks("").is_empty() && !masks("*.txt").is_empty());
}
