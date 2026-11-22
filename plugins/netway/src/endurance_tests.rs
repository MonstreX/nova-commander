use super::*;

fn values(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_dialog_shows_and_gives_back_netboxs_defaults() {
    let params = CopyParams::default();
    let items = items(&params, Reconnect::default());
    assert_eq!(items.len(), OK + 2);
    assert_eq!(items[SMART].state, 1);
    assert_eq!(items[THRESHOLD].text, "100");
    assert_eq!(items[AUTO_CHECK].state, 1);
    assert_eq!(items[AUTO_SECONDS].text, "5");
    assert_eq!(items[RETRIES_EDIT].text, "2");
    assert_eq!(items[STALL_EDIT].text, "10");
    assert!(items[ALL].group && !items[SMART].group && !items[OFF].group);
    let (back, reconnect) = read_values(&values(&items), &params);
    assert_eq!((back, reconnect), (params, Reconnect::default()));
}

#[test]
fn every_resume_and_no_reconnect_read_back() {
    let params = CopyParams::default();
    let mut read = values(&items(&params, Reconnect::default()));
    read[SMART].state = 0;
    read[OFF].state = 1;
    read[AUTO_CHECK].state = 0;
    read[STALL_EDIT].text = "30".to_string();
    let (back, reconnect) = read_values(&read, &params);
    assert_eq!(back.resume, Resume::Off);
    assert_eq!(reconnect.auto, 0, "asked, not automatic");
    assert_eq!(reconnect.stall, 30_000);
    read[OFF].state = 0;
    read[ALL].state = 1;
    read[AUTO_CHECK].state = 1;
    read[AUTO_SECONDS].text = "15".to_string();
    read[RETRIES_EDIT].text = "150".to_string();
    let (back, reconnect) = read_values(&read, &params);
    assert_eq!(back.resume, Resume::On);
    assert_eq!((reconnect.auto, reconnect.retries), (15_000, 99));
}
