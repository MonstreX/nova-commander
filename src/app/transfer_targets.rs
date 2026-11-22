//! The copy dialog's target line read as FAR reads it: several destinations, quotes.

/// FAR's several destinations: split at `;` and `,` outside double quotes; quotes removed, spaces
/// around a part dropped (those inside quotes stay), empty parts dropped.
pub fn split_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut current = String::new();
    // How much of `current` stays: not the spaces after the last thing that counts.
    let mut kept = 0;
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' | ';' if !quoted => {
                current.truncate(kept);
                targets.push(std::mem::take(&mut current));
                kept = 0;
            }
            _ if !quoted && ch.is_whitespace() && current.is_empty() => {}
            _ => {
                current.push(ch);
                if quoted || !ch.is_whitespace() {
                    kept = current.len();
                }
            }
        }
    }
    current.truncate(kept);
    targets.push(current);
    targets.retain(|target| !target.is_empty());
    targets
}

pub(in crate::app) fn unquote(text: &str) -> String {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| text.trim_matches('"'))
        .to_string()
}
