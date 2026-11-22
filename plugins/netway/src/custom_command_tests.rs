use super::*;

fn data() -> Data {
    Data {
        folder: "/home/u".to_string(),
        host: "srv".to_string(),
        user: "u".to_string(),
        password: "pw".to_string(),
    }
}

#[test]
fn the_patterns_are_put_in() {
    let files = ["a b.txt".to_string(), "c.txt".to_string()];
    assert_eq!(
        expanded("chmod 644 \"!\"", &data(), ("a b.txt", &files), &[]),
        "chmod 644 \"a b.txt\""
    );
    assert_eq!(
        expanded("tar czf x.tgz !&", &data(), ("", &files), &[]),
        "tar czf x.tgz \"a b.txt\" \"c.txt\""
    );
    assert_eq!(
        expanded("echo !! !/ !@ !U !P", &data(), ("f", &files), &[]),
        "echo ! /home/u/ srv u pw"
    );
}

#[test]
fn prompts_are_found_and_answered() {
    let command = "grep !?Pattern\\??x! ! && echo !?Count?3!";
    assert_eq!(
        prompts(command),
        [
            Prompt {
                prompt: "Pattern?".to_string(),
                default: "x".to_string()
            },
            Prompt {
                prompt: "Count".to_string(),
                default: "3".to_string()
            },
        ]
    );
    let answers = ["needle".to_string()];
    assert_eq!(
        expanded(command, &data(), ("f.txt", &[]), &answers),
        "grep needle f.txt && echo 3"
    );
}

#[test]
fn a_list_runs_once() {
    assert!(takes_list("ls -l !&"));
    assert!(!takes_list("echo !!& !"));
    assert!(!takes_list("rm \"!\""));
}
