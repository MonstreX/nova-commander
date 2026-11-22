use super::*;

fn record(pid: u32, name: &str, created: i64) -> ProcessRecord {
    ProcessRecord {
        pid,
        parent: 4,
        name: name.to_string(),
        threads: 3,
        base_priority: 8,
        session: 1,
        created,
        user_time: 1_000_000,
        working_set: 4096,
        page_file: 1000,
        page_faults: 10,
        ..ProcessRecord::default()
    }
}

fn details(pid: u32) -> Option<ProcessDetails> {
    Some(ProcessDetails {
        path: Some(format!("C:\\{pid}.exe")),
        command_line: Some(format!("{pid}.exe /x")),
        wow64: pid == 200,
        gdi_objects: 5,
        user_objects: 6,
        created: Some(1),
        owner: Some(Owner {
            user: "me".to_string(),
            domain: "HOME".to_string(),
            sid: "S-1-5-21".to_string(),
        }),
    })
}

/// A table of `records` read once, the details' calls counted.
fn first(records: &[ProcessRecord], asked: &mut Vec<u32>) -> Table {
    let mut ask = |pid| {
        asked.push(pid);
        details(pid)
    };
    Table::default().next(records, Instant::now(), 2, &mut ask)
}

/// The idle process, System and a process of the user's, read once.
fn three(asked: &mut Vec<u32>) -> Table {
    first(
        &[
            record(0, "", 0),
            record(4, "System", 5),
            record(100, "a.exe", 7),
        ],
        asked,
    )
}

#[test]
fn each_process_is_read_with_total_first() {
    let mut asked = Vec::new();
    let table = three(&mut asked);
    let keys: Vec<String> = table.processes.iter().map(Process::key).collect();
    assert_eq!(keys, ["_Total", "Idle 0", "System 4", "a.exe 100"]);
    assert_eq!(asked, [100], "FAR opens the processes above 8 only");
    let total = &table.processes[0];
    assert!(total.is_total());
    assert_eq!(total.threads, 9);
    assert_eq!(total.counters[counters::WORKING_SET], 3 * 4096);
    assert_eq!(total.bits, None);
    assert_eq!(table.by_key("System 4").unwrap().bits, Some(DEFAULT_BITS));
}

#[test]
fn an_opened_process_has_its_details() {
    let table = three(&mut Vec::new());
    let a = table.by_key("a.exe 100").unwrap();
    assert_eq!(a.path.as_deref(), Some(r"C:\100.exe"));
    assert_eq!(a.command_line.as_deref(), Some("100.exe /x"));
    assert_eq!((a.gdi, a.user, a.bits), (5, 6, Some(DEFAULT_BITS)));
    assert_eq!(a.owner.as_deref(), Some(r"HOME\me:1"));
    assert_eq!(a.created, 7, "the record's start before the opened one's");
    assert_eq!(a.size(), 4096 + 1000);
}

#[test]
fn an_entry_is_found_by_its_whole_name() {
    let table = three(&mut Vec::new());
    assert_eq!(table.by_key("_Total").map(|total| total.pid), Some(TOTAL));
    assert!(table.by_key("a.exe 101").is_none());
    assert!(table.by_key("b.exe 100").is_none(), "the name must fit too");
    assert!(table.by_key("a.exe").is_none());
}

#[test]
fn a_wow64_process_is_32_bit() {
    let table = first(&[record(200, "old.exe", 1)], &mut Vec::new());
    assert_eq!(table.by_key("old.exe 200").unwrap().bits, Some(32));
}

#[test]
fn a_process_not_opened_has_its_records_only() {
    let mut none = |_| None;
    let table = Table::default().next(&[record(300, "x.exe", 0)], Instant::now(), 1, &mut none);
    let x = &table.processes[1];
    assert_eq!((x.path.as_ref(), x.owner.as_ref()), (None, None));
    assert_eq!(x.bits, Some(DEFAULT_BITS));
    assert_eq!(x.started(), None, "no start known");
}

/// The second read: the per second values from the first, the details kept, not asked again; a number
/// back with another start is a new process.
#[test]
fn the_next_read_keeps_details_and_counts_since_the_first() {
    let mut asked = Vec::new();
    let mut table = first(
        &[record(100, "a.exe", 7), record(104, "b.exe", 9)],
        &mut asked,
    );
    let at = table.read_at.unwrap();
    table.processes[1].path = Some("kept".to_string());
    let mut later = record(100, "a.exe", 7);
    later.user_time += 20_000_000;
    later.page_faults += 50;
    let reused = record(104, "c.exe", 10);
    let mut ask = |pid| {
        asked.push(pid);
        details(pid)
    };
    let next = table.next(&[later, reused], at + Duration::from_secs(2), 2, &mut ask);
    assert_eq!(asked, [100, 104, 104]);
    let a = next.by_key("a.exe 100").unwrap();
    assert_eq!(a.path.as_deref(), Some("kept"));
    assert_eq!(
        a.results[0], 50,
        "2 s of processor time in 2 s on two processors"
    );
    assert_eq!(a.results[11], 25, "page faults per second");
    let c = next.by_key("c.exe 104").unwrap();
    assert_eq!(c.results[0], 0, "a new process has nothing per second yet");
    assert_eq!(
        next.processes[0].results[0], 50,
        "_Total's from its own sums"
    );
}

#[test]
fn the_start_is_a_time_from_1601() {
    let process = Process {
        created: 116_444_736_000_000_000 + 10_000_000,
        ..Process::default()
    };
    assert_eq!(process.started(), Some(UNIX_EPOCH + Duration::from_secs(1)));
}

#[test]
fn an_owner_without_a_domain_is_the_user_alone() {
    let owner = Owner {
        user: "SYSTEM".to_string(),
        ..Owner::default()
    };
    assert_eq!(owner_text(&owner, 0), "SYSTEM:0");
}
