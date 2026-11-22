use super::*;

fn values_of(items: &[DialogItem]) -> Vec<Value> {
    items
        .iter()
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_dialog_gives_its_command_and_params_back() {
    let params = TO_DIRECTORIES | SHOW_RESULTS | COPY_RESULTS;
    let items = apply_items("ls -l \"!\"", params);
    assert_eq!(items[OK].text, "OK");
    assert!(items[9].disabled, "the local command comes later");
    assert_eq!(
        applied(&values_of(&items)),
        ("ls -l \"!\"".to_string(), params)
    );
}

/// Against the tests' server: the selected items found recursively, the command run for each in the
/// panel's folder, its output collected.
#[test]
fn the_command_runs_on_the_server_for_the_items() {
    use crate::ssh::{Answer, Auth, Login, connect, trust_by};
    use crate::test_server::{PASSWORD, SHELL_USER, TestServer};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir_all(root.join("srv/docs/deep")).unwrap();
    std::fs::write(root.join("srv/a.txt"), "a").unwrap();
    std::fs::write(root.join("srv/docs/deep/b.txt"), "b").unwrap();
    let server = TestServer::start(&root);
    let runtime = crate::runtime::Runtime::new().unwrap();
    let login = Login {
        host: "127.0.0.1".to_string(),
        port: server.port,
        user: SHELL_USER.to_string(),
        auth: Auth::Password(PASSWORD.to_string()),
    };
    let known = temp.path().join("known_hosts");
    let trust = trust_by(|_| Answer::Keep);
    let connection = runtime.wait(connect(&login, &known, trust), &|| false);
    let connection = connection.unwrap().unwrap();
    let fs = crate::fs::Fs::scp(connection.commands(), &Default::default());
    let targets = ["a.txt".to_string(), "docs".to_string()];
    let found = runtime.wait(selected(&fs, "/srv", &targets, true), &|| false);
    let mut found = found.unwrap().unwrap();
    found.sort();
    assert_eq!(
        found,
        [
            ("a.txt".to_string(), false),
            ("docs".to_string(), true),
            ("docs/deep".to_string(), true),
            ("docs/deep/b.txt".to_string(), false),
        ]
    );
    let data = Data {
        folder: "/srv".to_string(),
        ..Data::default()
    };
    let lines = command_lines("echo \"!\"", &data, &found, (0, &[]));
    let commands = fs.commands().unwrap();
    let mut output = String::new();
    for line in lines {
        let ran = runtime
            .wait(commands.run(&line), &|| false)
            .unwrap()
            .unwrap();
        output.push_str(&ran.out);
    }
    assert_eq!(output, "a.txt\ndocs/deep/b.txt\n");
}

#[test]
fn the_command_runs_per_file_or_once_for_the_list() {
    let data = Data {
        folder: "/srv".to_string(),
        ..Data::default()
    };
    let items = [("a.txt".to_string(), false), ("docs".to_string(), true)];
    assert_eq!(
        command_lines("wc -c \"!\"", &data, &items, (0, &[])),
        ["cd \"/srv\" && wc -c \"a.txt\""]
    );
    assert_eq!(
        command_lines("du \"!\"", &data, &items, (TO_DIRECTORIES, &[])),
        ["cd \"/srv\" && du \"a.txt\"", "cd \"/srv\" && du \"docs\""]
    );
    assert_eq!(
        command_lines("ls !&", &data, &items, (TO_DIRECTORIES, &[])),
        ["cd \"/srv\" && ls \"a.txt\" \"docs\""]
    );
}
