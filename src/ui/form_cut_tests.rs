//! Labels longer than the dialog's box (a plugin's may be): cut at its right side, the frame kept.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

#[test]
fn a_label_or_a_toggles_label_longer_than_the_box_stops_before_its_frame() {
    let checked = |label: &str| Control::Check {
        label: label.to_string(),
        checked: false,
    };
    let radio = Control::Radio {
        label: "r".repeat(40),
        checked: true,
        group: true,
    };
    let items = vec![
        Item::new(5, 2, Control::Text("t".repeat(40))),
        Item::new(5, 3, checked(&"c".repeat(40))),
        Item::new(5, 4, radio),
        Item::new(
            5,
            5,
            Control::Tristate {
                label: "q".repeat(40),
                state: None,
                three: true,
            },
        ),
    ];
    // The box from column 0 to 29, its frame's right side at column 26.
    let form = Form::new("Cut", 30, 9, items);
    let mut terminal = Terminal::new(TestBackend::new(30, 9)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row = |y: u16| (0..30).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    let side = buffer[(26, 6)].symbol().to_string();
    assert_eq!(row(2).matches('t').count(), 21, "{}", row(2));
    assert_eq!(
        row(3).matches('c').count(),
        17,
        "after its mark: {}",
        row(3)
    );
    assert_eq!(row(4).matches('r').count(), 17, "{}", row(4));
    assert_eq!(row(5).matches('q').count(), 17, "{}", row(5));
    let marks: Vec<String> = (3..6)
        .map(|y| row(y).chars().skip(5).take(4).collect())
        .collect();
    assert_eq!(marks, ["[ ] ", "(•) ", "[?] "]);
    for y in 2..6 {
        assert_eq!(buffer[(26, y)].symbol(), side, "the frame: {}", row(y));
    }
}
