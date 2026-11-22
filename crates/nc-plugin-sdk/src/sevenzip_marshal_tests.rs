//! The marshal with threads of the test's own standing for the library's.

use std::sync::Arc;
use std::thread::ThreadId;

use super::*;

/// What the caller's side saw.
#[derive(Default)]
struct Seen {
    jobs: Vec<(u32, ThreadId)>,
}

/// Jobs asked from the call's thread and from threads it starts all run on the caller's thread, each
/// answer back to its asker; the call's answer comes back.
#[test]
fn jobs_run_on_the_callers_thread() {
    let marshal: Arc<Marshal<Seen>> = Arc::new(Marshal::new());
    let mut seen = Seen::default();
    let asker = Arc::clone(&marshal);
    let answer = marshal.serve(&mut seen, move || {
        let first = asker.ask(|seen: &mut Seen| {
            seen.jobs.push((1, std::thread::current().id()));
            10
        });
        let others: Vec<Option<u32>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (2..6)
                .map(|number| {
                    let asker = &asker;
                    scope.spawn(move || {
                        asker.ask(move |seen: &mut Seen| {
                            seen.jobs.push((number, std::thread::current().id()));
                            number * 10
                        })
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });
        (first, others)
    });
    assert_eq!(answer.0, Some(10));
    assert_eq!(answer.1, [Some(20), Some(30), Some(40), Some(50)]);
    let here = std::thread::current().id();
    assert_eq!(seen.jobs.len(), 5);
    assert!(seen.jobs.iter().all(|(_, thread)| *thread == here));
}

/// No call served: a job is not run; after a call neither.
#[test]
fn a_job_outside_a_call_is_refused() {
    let marshal: Marshal<Seen> = Marshal::new();
    assert_eq!(marshal.ask(|_: &mut Seen| 1), None);
    let mut seen = Seen::default();
    marshal.serve(&mut seen, || ());
    assert_eq!(marshal.ask(|_: &mut Seen| 1), None);
}

/// A job that panics answers none and every job after it is refused; its panic is carried to the
/// caller once the call ends, as the call's own panic is.
#[test]
fn panics_are_carried_to_the_caller() {
    let marshal: Arc<Marshal<Seen>> = Arc::new(Marshal::new());
    let mut seen = Seen::default();
    let asker = Arc::clone(&marshal);
    let answers = Arc::new(Mutex::new(Vec::new()));
    let told = Arc::clone(&answers);
    let carried = catch_unwind(AssertUnwindSafe(|| {
        marshal.serve(&mut seen, move || {
            let panicked = asker.ask(|_: &mut Seen| -> u32 { panic!("job") });
            let next = asker.ask(|seen: &mut Seen| {
                seen.jobs.push((2, std::thread::current().id()));
                2
            });
            told.lock().unwrap().extend([panicked, next]);
        });
    }));
    let payload = carried.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"job"));
    assert_eq!(*answers.lock().unwrap(), [None, None]);
    assert!(seen.jobs.is_empty(), "no job after the panic");
    let mut fresh = Seen::default();
    let asker = Arc::clone(&marshal);
    let again = marshal.serve(&mut fresh, move || asker.ask(|_: &mut Seen| 3));
    assert_eq!(again, Some(3), "the next call serves anew");
    let carried = catch_unwind(AssertUnwindSafe(|| {
        marshal.serve(&mut seen, || panic!("call"));
    }));
    assert!(carried.is_err());
    assert_eq!(marshal.ask(|_: &mut Seen| 1), None);
}

/// A job's panic goes on before the call's own when both panic (the first panic is the one told).
#[test]
fn a_jobs_panic_goes_before_the_calls() {
    let marshal: Arc<Marshal<Seen>> = Arc::new(Marshal::new());
    let mut seen = Seen::default();
    let asker = Arc::clone(&marshal);
    let carried = catch_unwind(AssertUnwindSafe(|| {
        marshal.serve(&mut seen, move || {
            let _ = asker.ask(|_: &mut Seen| -> u32 { panic!("job") });
            panic!("call");
        });
    }));
    let payload = carried.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"job"));
}

/// Many threads asking at once: every job runs once, on the caller's thread, its answer back to its
/// own asker (one job at a time is handed over).
#[test]
fn jobs_asked_at_once_each_run_once() {
    const THREADS: u32 = 8;
    const ASKS: u32 = 300;
    let marshal: Arc<Marshal<Seen>> = Arc::new(Marshal::new());
    let mut seen = Seen::default();
    let asker = Arc::clone(&marshal);
    let answers = marshal.serve(&mut seen, move || {
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|thread| {
                    let asker = &asker;
                    scope.spawn(move || {
                        (0..ASKS)
                            .map(|ask| {
                                let number = thread * ASKS + ask;
                                let answer = asker.ask(move |seen: &mut Seen| {
                                    seen.jobs.push((number, std::thread::current().id()));
                                    number
                                });
                                answer == Some(number)
                            })
                            .filter(|right| *right)
                            .count()
                    })
                })
                .collect();
            (handles.into_iter())
                .map(|handle| handle.join().unwrap())
                .sum::<usize>()
        })
    });
    let all = usize::try_from(THREADS * ASKS).unwrap();
    assert_eq!(answers, all, "every answer its asker's");
    let mut numbers: Vec<u32> = seen.jobs.iter().map(|(number, _)| *number).collect();
    numbers.sort_unstable();
    assert_eq!(
        numbers,
        (0..THREADS * ASKS).collect::<Vec<_>>(),
        "each once"
    );
    let here = std::thread::current().id();
    assert!(seen.jobs.iter().all(|(_, thread)| *thread == here));
}

/// Calls served one after another each answer their own jobs; a job's answer is its own whatever
/// other jobs ran before it.
#[test]
fn each_call_serves_its_own_jobs() {
    let marshal: Arc<Marshal<Seen>> = Arc::new(Marshal::new());
    for round in 0..3u32 {
        let mut seen = Seen::default();
        let asker = Arc::clone(&marshal);
        let answers = marshal.serve(&mut seen, move || {
            (0..4)
                .map(|at| {
                    asker.ask(move |seen: &mut Seen| {
                        seen.jobs.push((at, std::thread::current().id()));
                        round * 10 + at
                    })
                })
                .collect::<Vec<_>>()
        });
        let expected: Vec<Option<u32>> = (0..4).map(|at| Some(round * 10 + at)).collect();
        assert_eq!(answers, expected);
        let order: Vec<u32> = seen.jobs.iter().map(|(at, _)| *at).collect();
        assert_eq!(order, [0, 1, 2, 3]);
    }
}
