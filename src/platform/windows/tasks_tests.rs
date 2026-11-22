use super::*;

/// This test's own image: its full path, as FAR's F2 shows it.
#[test]
fn a_process_image_is_its_full_path() {
    let image = process_image(std::process::id());
    let own = std::env::current_exe().unwrap();
    assert!(
        image.eq_ignore_ascii_case(&own.to_string_lossy()),
        "{image} vs {}",
        own.display()
    );
    assert_eq!(process_image(u32::MAX - 2), "", "none such");
}

/// A process of the test's own ended; one that is none refused.
#[test]
fn a_process_is_ended() {
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "pause"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    terminate_process(child.id()).unwrap();
    let status = child.wait().unwrap();
    assert_eq!(
        status.code().and_then(|code| u32::try_from(code).ok()),
        Some(ERROR_PROCESS_ABORTED)
    );
    assert!(terminate_process(u32::MAX - 2).is_err());
}

/// The windows are gone through; each listed one has its process.
#[test]
fn the_task_windows_have_their_processes() {
    let windows = task_windows().unwrap();
    assert!(windows.iter().all(|one| one.pid != 0 && one.window != 0));
}
