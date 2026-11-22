use super::*;

fn read(prefix: &str, text: &str) -> Option<Command> {
    parse(prefix, text, " ", (Catch::Both, Screen::Hide))
}

#[test]
fn an_object_is_taken_as_it_is() {
    let command = read("VIEW", "  file.txt").unwrap();
    assert_eq!(command.prefix, "view");
    assert_eq!(command.object, "file.txt");
    assert!(!command.from);
    assert_eq!(read("goto", ""), None, "nothing: its syntax");
    assert_eq!(read("go", ""), None, "three letters at most: its syntax");
}

#[test]
fn edit_takes_its_line_and_column_in_brackets() {
    let command = read("edit", "[10,5] a.txt").unwrap();
    assert_eq!((command.line, command.column), (Some(10), Some(5)));
    assert_eq!(command.object, "a.txt");
    let line = read("edit", "[7] a.txt").unwrap();
    assert_eq!((line.line, line.column), (Some(7), None));
    let column = read("edit", "[,3] a.txt").unwrap();
    assert_eq!((column.line, column.column), (Some(0), Some(3)));
    let name = read("edit", r#""[1].txt""#).unwrap();
    assert_eq!(name.object, r#""[1].txt""#, "quoted: the name");
    let spaced = read("edit", " [1].txt").unwrap();
    assert_eq!(spaced.object, "[1].txt", "a space first: the name");
    assert_eq!(read("edit", "[-5] a.txt"), None, "below -1: its syntax");
    assert_eq!(
        read("edit", "[5] a,b.txt"),
        None,
        "a comma after the bracket, as FAR's"
    );
}

#[test]
fn a_line_from_a_variable_is_its_number() {
    let processors: i64 = std::env::var("NUMBER_OF_PROCESSORS")
        .unwrap()
        .parse()
        .unwrap();
    let command = read("edit", "[%NUMBER_OF_PROCESSORS%,] f.txt").unwrap();
    assert_eq!((command.line, command.column), (Some(processors), Some(0)));
}

#[test]
fn after_less_than_come_the_stream_the_mode_and_the_object() {
    let command = read("view", "<1> dir /b").unwrap();
    assert!(command.from);
    assert_eq!(
        (command.catch, command.screen),
        (Catch::Output, Screen::Hide)
    );
    assert_eq!(command.object, "dir /b");
    let both = read("edit", "<?+|C:\\x|dir").unwrap();
    assert_eq!((both.catch, both.screen), (Catch::Apart, Screen::Both));
    assert_eq!(both.object, "|C:\\x|dir");
    let plain = read("clip", "<dir").unwrap();
    assert_eq!(
        (plain.catch, plain.screen),
        (Catch::Both, Screen::Hide),
        "the defaults"
    );
    let goto = read("goto", " <list.txt").unwrap();
    assert!(goto.from);
    assert_eq!(goto.object, "list.txt");
}

#[test]
fn run_takes_its_file_before_the_command() {
    let command = read("run", r"out.txt < dir").unwrap();
    assert_eq!(command.run_file.as_deref(), Some("out.txt"));
    assert!(command.from);
    assert_eq!(command.object, "dir");
    assert_eq!(read("run", "dir"), None, "no command: nothing");
}

#[test]
fn the_settings_numbers_are_the_configurations_order() {
    for index in 0..4 {
        assert_eq!(Catch::of(index).index(), index);
    }
    for index in 0..3 {
        assert_eq!(Screen::of(index).index(), index);
    }
}
