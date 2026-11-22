use super::*;

#[test]
fn paths_are_quoted_as_winscp_quotes_them() {
    assert_eq!(quoted("/home/a b"), "\"/home/a b\"");
    assert_eq!(quoted("a\"b$c`d\\e"), "\"a\\\"b\\$c\\`d\\\\e\"");
}

#[test]
fn a_shell_runs_the_command_quoted() {
    assert_eq!(in_shell(None, "ls -la \"/a\""), "ls -la \"/a\"");
    assert_eq!(
        in_shell(Some("/bin/bash"), "ls -la \"/a\""),
        "/bin/bash -c \"ls -la \\\"/a\\\"\""
    );
}

#[test]
fn a_failure_says_its_error_output_first() {
    let output = Output {
        status: 1,
        out: "out\n".to_string(),
        err: "rm: cannot remove\n".to_string(),
    };
    assert_eq!(output.failure(), "rm: cannot remove");
    let silent = Output {
        status: 2,
        ..Output::default()
    };
    assert_eq!(silent.failure(), "Command failed with return code 2.");
}
