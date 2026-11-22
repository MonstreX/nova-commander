use super::*;

#[test]
fn the_switches_and_both_names_are_read() {
    let link = parse(r"/msg /n /s C:\a D:\b").unwrap();
    assert!(link.tell && link.keep_panels && link.symbolic);
    assert_eq!(
        (link.source.as_str(), link.destination.as_str()),
        (r"C:\a", r"D:\b")
    );
    let quoted = parse(r#""C:\my files" "D:\new place""#).unwrap();
    assert_eq!(quoted.source, r"C:\my files");
    assert_eq!(quoted.destination, r"D:\new place");
    assert_eq!(parse(r"C:\a"), None, "no destination: its syntax");
    let cyrillic = parse("файл.txt копия.txt").unwrap();
    assert_eq!(
        (cyrillic.source.as_str(), cyrillic.destination.as_str()),
        ("файл.txt", "копия.txt")
    );
}

#[test]
fn the_kind_follows_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("f.txt");
    std::fs::write(&file, "").unwrap();
    let plain = parse("a b").unwrap();
    assert_eq!(kind(&plain, temp.path()), Kind::Junction);
    assert_eq!(kind(&plain, &file), Kind::Hard);
    let symbolic = parse("/s a b").unwrap();
    assert_eq!(kind(&symbolic, temp.path()), Kind::SymbolicFolder);
    assert_eq!(kind(&symbolic, &file), Kind::SymbolicFile);
    for drive in [r"E:", r"E:\"] {
        let mount = parse(&format!(r"{drive} C:\cdrom")).unwrap();
        assert_eq!(kind(&mount, temp.path()), Kind::Mount, "{drive}");
    }
}

#[test]
fn a_destination_ending_in_a_separator_takes_the_sources_name() {
    let folder = Path::new(r"C:\work");
    let into = parse(r"C:\data\file.txt D:\links\").unwrap();
    assert_eq!(
        link_path(&into, Kind::Hard, folder),
        Path::new(r"D:\links\file.txt")
    );
    let drive = parse(r"E:\ C:\mounts\").unwrap();
    assert_eq!(
        link_path(&drive, Kind::Mount, folder),
        Path::new(r"C:\mounts\Disk_E")
    );
    let relative = parse(r"a.txt b.txt").unwrap();
    assert_eq!(
        link_path(&relative, Kind::Hard, folder),
        Path::new(r"C:\work\b.txt")
    );
}

#[test]
fn a_hard_link_and_a_junction_are_made_on_the_way_never_over_anything() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("f.txt");
    std::fs::write(&file, "x").unwrap();
    let hard = temp.path().join("new").join("g.txt");
    make(Kind::Hard, &file, &hard).unwrap();
    assert_eq!(std::fs::read_to_string(&hard).unwrap(), "x");
    assert!(make(Kind::Hard, &file, &hard).is_err(), "it exists");
    let folder = temp.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("in.txt"), "in").unwrap();
    let junction = temp.path().join("junction");
    make(Kind::Junction, &folder, &junction).unwrap();
    assert_eq!(
        std::fs::read_to_string(junction.join("in.txt")).unwrap(),
        "in"
    );
    std::fs::remove_dir(&junction).unwrap();
}
