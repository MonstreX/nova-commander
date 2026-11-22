use std::path::Path;

use super::*;

/// F5's start takes the kept answers; the viewer's overwrites into the folder given, Find file's and the
/// quick view's let errors be; going to the folder only for archives of a disk's panel at once.
#[test]
fn an_extraction_starts_as_arclites() {
    let settings = ExtractSettings {
        ignore_errors: false,
        overwrite: Overwrite::Skip,
        separate_dir: Tri::True,
        open_dir: true,
    };
    let f5 = ExtractOptions::starting(PathBuf::from(r"C:\out"), GetMode::default(), &settings);
    assert_eq!(f5.dst_dir, Path::new(r"C:\out"));
    assert_eq!(
        (f5.overwrite, f5.separate_dir),
        (Overwrite::Skip, Tri::True)
    );
    assert_eq!(f5.move_files, Tri::Undef, "no archive is changed yet");
    assert_eq!(f5.open_dir, Tri::Undef, "not offered for F5");
    assert!(!f5.ignore_errors && f5.password.is_empty() && !f5.delete_archive);
    let view = GetMode {
        silent: true,
        view: true,
        ..GetMode::default()
    };
    let viewed = ExtractOptions::starting(PathBuf::from(r"C:\t"), view, &settings);
    assert_eq!(
        (viewed.overwrite, viewed.separate_dir),
        (Overwrite::Overwrite, Tri::False)
    );
    let quick = GetMode {
        quick_view: true,
        ..GetMode::default()
    };
    assert!(ExtractOptions::starting(PathBuf::new(), quick, &settings).ignore_errors);
    let top = GetMode {
        top_level: true,
        ..GetMode::default()
    };
    assert_eq!(
        ExtractOptions::starting(PathBuf::new(), top, &settings).open_dir,
        Tri::True
    );
}

/// The kept numbers are arclite's; a check box's states read as its tri-state.
#[test]
fn the_answers_are_kept_as_arclites_numbers() {
    assert_eq!(Overwrite::of(3), Overwrite::Rename);
    assert_eq!(Overwrite::of(99), Overwrite::Ask);
    assert_eq!(Tri::of(0), Tri::True);
    assert_eq!(Tri::of(1), Tri::False);
    assert_eq!(Tri::of(7), Tri::Undef);
    assert_eq!(Tri::of_state(1), Tri::True);
    assert_eq!(Tri::of_state(0), Tri::False);
    assert_eq!(Tri::of_state(2), Tri::Undef);
    assert_eq!(Tri::Undef.check(), None);
    assert_eq!(Tri::True.check(), Some(true));
    assert_eq!(ExtractSettings::default().separate_dir, Tri::Undef);
}

/// Every answer read back from the number it is kept as (arclite's `OverwriteAction` order); one
/// unknown, negative or past the last, as Ask.
#[test]
fn every_overwrite_number_reads_back() {
    let all = [
        Overwrite::Ask,
        Overwrite::Overwrite,
        Overwrite::Skip,
        Overwrite::Rename,
        Overwrite::Append,
        Overwrite::OverwriteCase,
    ];
    for (number, answer) in (0..).zip(all) {
        assert_eq!(answer as i64, number, "{answer:?} kept as");
        assert_eq!(Overwrite::of(number), answer, "{number} read as");
    }
    assert_eq!(Overwrite::of(6), Overwrite::Ask);
    assert_eq!(Overwrite::of(-1), Overwrite::Ask);
}

/// arclite's `TriState` numbers kept (0 true, 1 false, 2 undefined); a check box's states are FAR's
/// (0 off, 1 on, 2 the third); each back as its check box shows it.
#[test]
fn the_tri_state_numbers_are_arclites_and_the_boxes_fars() {
    for (number, tri) in [(0, Tri::True), (1, Tri::False), (2, Tri::Undef)] {
        assert_eq!(tri as i64, number);
        assert_eq!(Tri::of(number), tri);
    }
    assert_eq!(Tri::of(-1), Tri::Undef);
    assert_eq!(Tri::of_state(3), Tri::Undef);
    assert_eq!(Tri::of_state(-1), Tri::Undef);
    assert_eq!(Tri::False.check(), Some(false));
    for tri in [Tri::True, Tri::False, Tri::Undef] {
        let state = match tri.check() {
            Some(true) => 1,
            Some(false) => 0,
            None => 2,
        };
        assert_eq!(Tri::of_state(state), tri, "{tri:?} through its box");
    }
    let defaults = ExtractSettings::default();
    assert!(!defaults.ignore_errors && !defaults.open_dir);
    assert_eq!(defaults.overwrite, Overwrite::Ask);
}

/// The kept "Ignore errors" goes into F5's start; the editor's overwrites as the viewer's; Find file
/// lets errors be, the viewer does not; going to the folder offered off when not kept so, and not
/// offered at all without the dialog even for archives of a disk's panel.
#[test]
fn each_mode_starts_as_arclite_starts_it() {
    let kept = ExtractSettings {
        ignore_errors: true,
        overwrite: Overwrite::Append,
        separate_dir: Tri::False,
        open_dir: false,
    };
    let f5 = ExtractOptions::starting(PathBuf::new(), GetMode::default(), &kept);
    assert!(f5.ignore_errors, "kept");
    assert_eq!(
        (f5.overwrite, f5.separate_dir),
        (Overwrite::Append, Tri::False)
    );
    assert!(!f5.disable_delete_archive);
    let top = GetMode {
        top_level: true,
        ..GetMode::default()
    };
    let shift_f2 = ExtractOptions::starting(PathBuf::new(), top, &kept);
    assert_eq!(shift_f2.open_dir, Tri::False, "offered, not kept");
    let quiet_top = GetMode {
        silent: true,
        top_level: true,
        ..GetMode::default()
    };
    let opening = ExtractSettings {
        open_dir: true,
        ..ExtractSettings::default()
    };
    let silent = ExtractOptions::starting(PathBuf::new(), quiet_top, &opening);
    assert_eq!(silent.open_dir, Tri::Undef, "no dialog: not offered");
    assert_eq!(silent.overwrite, Overwrite::Overwrite);
    let defaults = ExtractSettings::default();
    let edit = GetMode {
        edit: true,
        ..GetMode::default()
    };
    let edited = ExtractOptions::starting(PathBuf::new(), edit, &defaults);
    assert_eq!(
        (edited.overwrite, edited.separate_dir),
        (Overwrite::Overwrite, Tri::False)
    );
    assert!(!edited.ignore_errors, "the editor tells its failures");
    let find = GetMode {
        find: true,
        ..GetMode::default()
    };
    let found = ExtractOptions::starting(PathBuf::new(), find, &defaults);
    assert!(found.ignore_errors, "Find file tells none");
    assert_eq!(found.overwrite, Overwrite::Overwrite);
    let f5 = ExtractOptions::starting(PathBuf::new(), GetMode::default(), &defaults);
    assert!(!f5.ignore_errors);
    assert_eq!(
        (f5.overwrite, f5.separate_dir),
        (Overwrite::Ask, Tri::Undef)
    );
}
