//! FAR's column types and widths read as `DeserialiseViewSettings` reads them.

use super::*;

fn kinds(columns: &[ColumnDef]) -> Vec<ColumnKind> {
    columns.iter().map(|column| column.kind).collect()
}

fn widths(columns: &[ColumnDef]) -> Vec<usize> {
    columns.iter().map(|column| column.width).collect()
}

#[test]
fn fars_modes_are_the_modes_of_old() {
    let modes = PanelModes::default();
    let detailed = modes.spec(ViewMode::Detailed);
    use ColumnKind::*;
    assert_eq!(
        kinds(&detailed.panel),
        [
            Name,
            Size,
            Allocated,
            WriteTime,
            CreationTime,
            AccessTime,
            Attributes
        ]
    );
    assert_eq!(widths(&detailed.panel), [0, 6, 6, 14, 14, 14, 0]);
    assert!(detailed.full_screen && detailed.align_extensions);
    let alternative = modes.spec(ViewMode::AlternativeFull);
    assert!(alternative.panel[0].flags.mark);
    assert!(alternative.panel[1].flags.group_digits);
    assert_eq!(widths(&alternative.panel), [0, 10, 0]);
    assert!(alternative.status[0].flags.right_align);
    let medium = modes.spec(ViewMode::Medium);
    assert_eq!(kinds(&medium.panel), [Name, Name]);
    assert_eq!(kinds(&medium.status), [Name, Size, Date, Time]);
    assert!(!medium.align_extensions);
    assert!(modes.spec(ViewMode::Brief).align_extensions);
    assert_eq!(kinds(&modes.spec(ViewMode::Wide).panel), [Name, Size]);
    assert_eq!(kinds(&modes.spec(ViewMode::Full).status), [Name]);
}

#[test]
fn modifiers_follow_the_letter_of_the_type() {
    let made = columns("NMDORFN,SCEFT,PE,DMBM,DEB,XR,DC", "");
    let name = made[0].flags;
    assert!(name.mark && name.mark_dynamic && name.name_only);
    assert!(name.right_align && name.right_align_force && name.no_extension);
    let size = made[1].flags;
    assert!(size.group_digits && size.economic && size.float && size.thousand);
    assert_eq!(made[2].kind, ColumnKind::Allocated);
    assert!(made[2].flags.economic);
    assert_eq!(made[3].kind, ColumnKind::WriteTime);
    assert!(made[3].flags.brief && made[3].flags.month);
    assert_eq!(made[4].kind, ColumnKind::ChangeTime);
    assert!(made[4].flags.brief && !made[4].flags.month);
    assert_eq!(made[5].kind, ColumnKind::Extension);
    assert!(made[5].flags.right_align);
    assert_eq!(made[6].kind, ColumnKind::CreationTime);
    assert_eq!(made[6].flags, ColumnFlags::default());
}

#[test]
fn an_unknown_type_is_left_out_and_one_not_drawn_keeps_its_width() {
    let unknown = columns("N,Q,S", "0,7,9");
    assert_eq!(kinds(&unknown), [ColumnKind::Name, ColumnKind::Size]);
    assert_eq!(widths(&unknown), [0, 7], "the unknown type takes no width");
    let not_drawn = columns("N,Z,O,LN,F,G,C0,<Mine>,S", "0,1,2,3,4,5,6,7,9");
    assert_eq!(
        kinds(&not_drawn),
        [
            ColumnKind::Name,
            ColumnKind::Description,
            ColumnKind::Custom(0),
            ColumnKind::Size
        ]
    );
    assert_eq!(
        widths(&not_drawn),
        [0, 1, 6, 9],
        "owners, links, streams, plugins' named ones take theirs"
    );
}

#[test]
fn widths_are_digits_percent_or_the_default() {
    let made = columns("N,S,D,T", "40%,x,,5");
    assert_eq!(widths(&made), [40, 0, 0, 5]);
    assert!(made[0].percent && !made[3].percent);
    let empty = columns("N,S,D", "-1,-0,-");
    assert_eq!(
        empty.iter().map(|column| column.empty).collect::<Vec<_>>(),
        [true, false, false],
        "a negative width is an empty column; -0 and - are the default (FAR's `int`)"
    );
    assert_eq!(widths(&empty), [0, 0, 0]);
    let short = columns("N,S", "3");
    assert_eq!(widths(&short), [3, 0], "a missing width is the default");
    assert_eq!(
        columns("", ""),
        [ColumnDef {
            kind: ColumnKind::Name,
            flags: ColumnFlags::default(),
            width: 0,
            percent: false,
            empty: false,
        }],
        "no column at all is one name column"
    );
}

#[test]
fn a_brief_or_month_date_changes_the_default_width() {
    let made = columns("DM,DMB,DMM,DMBM,D,T", "");
    let defaults: Vec<usize> = made.iter().map(ColumnDef::default_width).collect();
    assert_eq!(defaults, [14, 11, 15, 12, 8, 5]);
}

#[test]
fn the_modes_come_from_the_settings_and_are_saved() {
    let temp = tempfile::tempdir().unwrap();
    let mut modes = settings::PanelModes::far_defaults();
    modes.brief.columns = "N,S".to_string();
    modes.brief.widths = "0,6".to_string();
    modes.brief.folder_upper_case = true;
    let saved = settings::Settings {
        panel_modes: Some(modes.clone()),
        ..settings::Settings::default()
    };
    let app = super::super::App::new(temp.path(), 100, 40, &saved);
    let brief = app.panel_modes.spec(ViewMode::Brief);
    assert_eq!(kinds(&brief.panel), [ColumnKind::Name, ColumnKind::Size]);
    assert!(brief.folder_upper_case);
    assert_eq!(app.to_settings().panel_modes, Some(modes));
    let fresh = super::super::App::new(temp.path(), 100, 40, &settings::Settings::default());
    assert_eq!(
        fresh.to_settings().panel_modes,
        Some(settings::PanelModes::far_defaults()),
        "none kept: FAR's"
    );
}

#[test]
fn a_c_without_a_number_is_a_name_and_a_width_past_an_int_the_default() {
    let made = columns("CX,C5,C1X,C99,S", "4,5,6,7,99999999999999999999");
    use ColumnKind::*;
    assert_eq!(
        kinds(&made),
        [Name, Custom(5), Custom(1), Custom(99), Size],
        "the digits it starts with"
    );
    assert_eq!(
        widths(&made),
        [4, 5, 6, 7, 0],
        "FAR's int cannot hold the size's width"
    );
    assert_eq!(kinds(&columns("C100,S", "")), [Size], "past C99: not FAR's");
    assert_eq!(widths(&columns("S", "2147483647")), [2_147_483_647]);
}

#[test]
fn a_providers_texts_and_description_are_columns_titled_by_their_place() {
    let titles = ["One", "owner", "Two", "", "Three"].map(|title| Some(title.to_string()));
    let (made, made_titles) = titled_columns("N,O,C0,Z,C12", "0,3,6,0,4", &titles);
    use ColumnKind::*;
    assert_eq!(kinds(&made), [Name, Custom(0), Description, Custom(12)]);
    assert_eq!(widths(&made), [0, 6, 0, 4]);
    assert_eq!(
        made_titles,
        [
            Some("One".to_string()),
            Some("Two".to_string()),
            Some("".to_string()),
            Some("Three".to_string())
        ],
        "the owners' title leaves with its column, an empty one stays"
    );
    let (_, short) = titled_columns("N,S", "", &[Some("Only".to_string())]);
    assert_eq!(
        short,
        [Some("Only".to_string()), None],
        "a missing title: the kind's"
    );
    assert_eq!(
        (Custom(3).title(), Description.title()),
        ("", "Description")
    );
    let defaults: Vec<usize> = (columns("C1,Z", "").iter())
        .map(ColumnDef::default_width)
        .collect();
    assert_eq!(defaults, [0, 0], "they share the room left");
}

/// A provider's mode of a name and its first text.
fn own_mode() -> vfs::PanelMode {
    vfs::PanelMode {
        columns: "N,C0".to_string(),
        widths: "0,5".to_string(),
        titles: vec![None, Some("Kind".to_string())],
        ..vfs::PanelMode::default()
    }
}

/// The program's modes with Full at every flag on.
fn all_flags_full() -> PanelModes {
    let mut kept = settings::PanelModes::far_defaults();
    kept.full.full_screen = true;
    kept.full.align_extensions = true;
    kept.full.folder_align_extensions = true;
    kept.full.folder_upper_case = true;
    kept.full.files_lower_case = true;
    kept.full.upper_to_lower_case = true;
    PanelModes::make(kept)
}

/// [`own_mode`] with every flag on.
fn flagged_mode() -> vfs::PanelMode {
    vfs::PanelMode {
        full_screen: true,
        align_extensions: true,
        case_conversion: true,
        detailed_status: true,
        ..own_mode()
    }
}

#[test]
fn a_providers_mode_takes_its_columns_and_flags_over_the_programs() {
    let modes = all_flags_full();
    let base = modes.spec(ViewMode::Full);
    use ColumnKind::*;
    let made = ModeSpec::of_provider(&own_mode(), base);
    assert_eq!(made.name, base.name);
    assert_eq!(kinds(&made.panel), [Name, Custom(0)]);
    assert_eq!(made.titles, [None, Some("Kind".to_string())]);
    assert!(
        !made.full_screen && !made.align_extensions,
        "its flags, not the program's"
    );
    assert!(
        made.folder_align_extensions,
        "the program's: a provider has no say"
    );
    assert!(
        !made.folder_upper_case && !made.files_lower_case && !made.upper_to_lower_case,
        "names as given"
    );
    let made = ModeSpec::of_provider(&flagged_mode(), base);
    assert!(made.full_screen && made.align_extensions);
    assert!(made.folder_upper_case && made.files_lower_case && made.upper_to_lower_case);
    let cased_medium = ModeSpec::of_provider(&flagged_mode(), modes.spec(ViewMode::Medium));
    assert!(
        !cased_medium.folder_upper_case && !cased_medium.files_lower_case,
        "the cases of the program's mode at that key"
    );
}

#[test]
fn a_providers_status_line_is_its_own_or_the_name_or_the_detailed_one() {
    let modes = all_flags_full();
    let base = modes.spec(ViewMode::Full);
    use ColumnKind::*;
    let made = ModeSpec::of_provider(&own_mode(), base);
    assert_eq!(kinds(&made.status), [Name], "no status columns: the name");
    assert!(made.status[0].flags.right_align);
    let made = ModeSpec::of_provider(&flagged_mode(), base);
    assert_eq!(kinds(&made.status), [Name, Size, Date, Time]);
    assert_eq!(widths(&made.status), [0, 8, 0, 5]);
    assert!(
        !made.status[1].flags.group_digits,
        "FAR's detailed status line: no digit groups"
    );
    let own_status = vfs::PanelMode {
        status_columns: "C0,NR".to_string(),
        status_widths: "6,0".to_string(),
        ..flagged_mode()
    };
    let made = ModeSpec::of_provider(&own_status, base);
    assert_eq!(
        kinds(&made.status),
        [Custom(0), Name],
        "its own status line first"
    );
    let half = vfs::PanelMode {
        status_columns: "C0".to_string(),
        ..flagged_mode()
    };
    let made = ModeSpec::of_provider(&half, base);
    assert_eq!(
        kinds(&made.status),
        [Name, Size, Date, Time],
        "columns without widths are none, as FAR's"
    );
}

#[test]
fn a_panel_shows_a_providers_mode_at_its_key_and_the_programs_elsewhere() {
    use std::borrow::Cow;
    let modes = PanelModes::default();
    let wide = vfs::PanelMode {
        columns: "N,Z".to_string(),
        ..own_mode()
    };
    let look = vfs::PanelLook {
        title: None,
        modes: vec![
            Some(own_mode()),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(wide),
        ]
        .into(),
        ..vfs::PanelLook::default()
    };
    use ColumnKind::*;
    let alternative = modes.spec_in(ViewMode::AlternativeFull, &look);
    assert!(matches!(alternative, Cow::Owned(_)), "Ctrl+0 is its first");
    assert_eq!(kinds(&alternative.panel), [Name, Custom(0)]);
    let seventh = modes.spec_in(ViewMode::Provider(7), &look);
    assert_eq!(kinds(&seventh.panel), [Name, Description]);
    for mode in [
        ViewMode::Brief,
        ViewMode::Medium,
        ViewMode::Detailed,
        ViewMode::Custom(0),
    ] {
        assert!(
            matches!(modes.spec_in(mode, &look), Cow::Borrowed(spec) if spec == modes.spec(mode)),
            "{mode:?}: the program's"
        );
    }
    assert_eq!(
        *modes.spec_in(ViewMode::Provider(6), &look),
        *modes.spec(ViewMode::Medium),
        "none there: Medium"
    );
    assert_eq!(
        *modes.spec_in(ViewMode::AlternativeFull, &vfs::PanelLook::default()),
        *modes.spec(ViewMode::AlternativeFull),
        "a disk's panel"
    );
}

#[test]
fn the_programs_modes_on_a_providers_panel_take_its_flags() {
    let modes = PanelModes::make({
        let mut kept = settings::PanelModes::far_defaults();
        kept.medium.folder_upper_case = true;
        kept
    });
    let base = modes.spec(ViewMode::Medium);
    let look = vfs::PanelLook {
        names_only: true,
        right_align_names: true,
        preserve_case: true,
        ..vfs::PanelLook::default()
    };
    let shown = modes.spec_in(ViewMode::Medium, &look);
    assert!(
        (shown.panel.iter()).all(|column| column.flags.name_only && column.flags.right_align),
        "FAR's NAMEONLY and RIGHTALIGN on the name columns"
    );
    assert!(!shown.folder_upper_case, "OPIF_SHOWPRESERVECASE");
    assert!(
        base.folder_upper_case && !base.panel[0].flags.name_only,
        "the program's mode stays"
    );
    assert_eq!(
        *modes.spec_in(ViewMode::Medium, &vfs::PanelLook::default()),
        *base,
        "no flags: the program's"
    );
    let names_only = vfs::PanelLook {
        names_only: true,
        ..vfs::PanelLook::default()
    };
    let size = (modes.spec_in(ViewMode::Full, &names_only).panel.iter())
        .find(|column| column.kind == ColumnKind::Size)
        .copied();
    assert!(
        size.is_some_and(|column| !column.flags.name_only),
        "only name columns"
    );
}

#[test]
fn a_view_shows_names_only_by_the_providers_flag_or_its_own_modes_name_column() {
    let own = |columns: &str| vfs::PanelMode {
        columns: columns.to_string(),
        ..vfs::PanelMode::default()
    };
    let look = vfs::PanelLook {
        modes: vec![None, Some(own("S,NO")), Some(own("N,S"))].into(),
        names_only: true,
        ..vfs::PanelLook::default()
    };
    let mut kept = settings::PanelModes::far_defaults();
    kept.custom.push(settings::PanelMode {
        columns: "NO,S".to_string(),
        ..settings::PanelMode::default()
    });
    let modes = PanelModes::make(kept);
    let names_only = |mode: ViewMode, look: &vfs::PanelLook| modes.spec_in(mode, look).names_only();
    assert!(
        names_only(ViewMode::Brief, &look),
        "its mode's `O` on its first name column"
    );
    assert!(
        !names_only(ViewMode::Medium, &look),
        "its mode without `O`: whole paths"
    );
    assert!(
        names_only(ViewMode::Full, &look),
        "the program's mode: the flag"
    );
    let plain = vfs::PanelLook::default();
    assert!(!names_only(ViewMode::Full, &plain));
    assert!(
        names_only(ViewMode::Custom(0), &plain),
        "a user's mode with `O`"
    );
}
