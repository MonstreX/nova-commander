use super::*;

fn record() -> ProcessRecord {
    ProcessRecord {
        user_time: 30,
        kernel_time: 70,
        handles: 5,
        working_set: 4096,
        page_file: 1000,
        read_bytes: 10,
        write_bytes: 20,
        read_operations: 1,
        write_operations: 2,
        page_faults: 7,
        working_set_private: 99,
        ..ProcessRecord::default()
    }
}

#[test]
fn the_values_are_fars_counters_in_their_order() {
    let values = values(&record());
    assert_eq!(values[0], 100, "processor time: user and privileged");
    assert_eq!((values[1], values[2]), (70, 30));
    assert_eq!(values[3], 5);
    assert_eq!(values[WORKING_SET], 4096);
    assert_eq!(values[PAGE_FILE], 1000);
    assert_eq!(values[11], 7);
    assert_eq!((values[14], values[18]), (30, 3), "data: read and write");
    assert_eq!(values[22], 99);
    assert_eq!(COUNTERS[14].per_second_column, "Byt/s");
}

#[test]
fn the_results_are_percent_per_second_or_as_they_are() {
    let old = values(&record());
    let mut later = record();
    // A second later: 2.5 s of processor time on 4 processors (62 %), 3000 more bytes read.
    later.user_time += 25_000_000;
    later.read_bytes += 3000;
    later.page_faults += 50;
    let now = values(&later);
    let shown = results(&now, Some(&old), 1000, 4);
    assert_eq!(shown[0], 62);
    assert_eq!(shown[2], 62);
    assert_eq!(shown[1], 0);
    assert_eq!(shown[15], 3000);
    assert_eq!(shown[11], 50);
    assert_eq!(shown[WORKING_SET], 4096, "as it is");
    let first = results(&now, None, 1000, 4);
    assert_eq!((first[0], first[15], first[WORKING_SET]), (0, 0, 4096));
    let fewer = results(&old, Some(&now), 1000, 4);
    assert_eq!(fewer[0], 0, "gone down: none");
}

#[test]
fn times_and_counts_are_shown_per_second_not_bytes() {
    assert!(per_second(0) && per_second(11) && per_second(18));
    assert!(!per_second(WORKING_SET) && !per_second(14) && !per_second(COUNT));
}
