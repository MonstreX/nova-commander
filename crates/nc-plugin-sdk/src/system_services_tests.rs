use super::*;

/// Windows always runs services in service hosts: some have names, and a service host is among them.
#[test]
fn running_services_are_found_by_their_processes() {
    let services = services_by_process();
    let named = (services.values().flatten())
        .filter(|service| !service.name.is_empty() && !service.display_name.is_empty())
        .count();
    assert!(named > 5, "{services:?}");
    assert!(
        (services.values().flatten()).any(|service| service.name.eq_ignore_ascii_case("EventLog")),
        "the event log's service"
    );
}
