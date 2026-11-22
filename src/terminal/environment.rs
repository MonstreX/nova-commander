//! The program's own environment changes (FAR's `set NAME=value` on the command line, ADR-0006): every
//! command started afterwards gets them. The process environment itself is left alone.

use crate::platform::process;

/// FAR's variable with the folder `pushd` remembered last.
const DIR_STACK_VARIABLE: &str = "FARDIRSTACK";

/// What the command line's own commands leave behind: variables set or removed with `set` in the order
/// given, the folders of `pushd`, the title of `title`, the code of `exit`.
#[derive(Debug, Default, Clone)]
pub struct Environment {
    /// Name as typed and the value; `None` means removed. Names compare as the system's do.
    changes: Vec<(String, Option<String>)>,
    dir_stack: Vec<String>,
    title: Option<String>,
    exit_code: i32,
    /// The code of an `exit` that asked first, while running tasks are being asked about.
    asked_exit_code: i32,
}

impl Environment {
    /// `set NAME=value`: `%VAR%` in the value is expanded first; an empty value removes the variable.
    pub fn set(&mut self, name: &str, value: &str) {
        let value = (!value.is_empty()).then(|| self.expand(value));
        self.store(name, value);
    }

    fn store(&mut self, name: &str, value: Option<String>) {
        self.changes.retain(|(known, _)| !same_name(known, name));
        self.changes.push((name.to_string(), value));
    }

    /// `pushd`: `dir` is remembered, taken literally, and shown in `FARDIRSTACK`.
    pub fn push_dir(&mut self, dir: &str) {
        self.dir_stack.push(dir.to_string());
        self.store(DIR_STACK_VARIABLE, Some(dir.to_string()));
    }

    /// `popd`: the folder remembered last, forgotten now.
    pub fn pop_dir(&mut self) -> Option<String> {
        let dir = self.dir_stack.pop()?;
        self.store(DIR_STACK_VARIABLE, self.dir_stack.last().cloned());
        Some(dir)
    }

    /// `clrd`.
    pub fn clear_dirs(&mut self) {
        self.dir_stack.clear();
        self.store(DIR_STACK_VARIABLE, None);
    }

    /// `title`: taken once by whoever shows it.
    pub fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }

    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    /// `exit`: the program's exit code.
    pub fn set_exit_code(&mut self, code: i32) {
        self.exit_code = code;
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// `exit` with tasks running: the code counts only once the quit is confirmed.
    pub fn ask_exit_code(&mut self, code: i32) {
        self.asked_exit_code = code;
    }

    /// The quit asked about was confirmed.
    pub fn confirm_exit_code(&mut self) {
        self.exit_code = self.asked_exit_code;
    }

    /// The variable's value as the next command will see it.
    pub fn get(&self, name: &str) -> Option<String> {
        match self
            .changes
            .iter()
            .find(|(known, _)| same_name(known, name))
        {
            Some((_, value)) => value.clone(),
            None => std::env::var(name).ok(),
        }
    }

    /// Replaces the variables in `text` (`%NAME%` in cmd) with their values; unknown names stay as
    /// written.
    pub fn expand(&self, text: &str) -> String {
        process::expand_variables(text, &|name| self.get(name))
    }

    /// The names of the variables the next command will have.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::env::vars_os()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .chain(self.changes.iter().map(|(name, _)| name.clone()))
            .filter(|name| !name.is_empty() && self.get(name).is_some())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| same_name(a, b));
        names
    }

    /// FAR's `ReloadEnvironment` (System > "Automatic update of environment variables"): each variable
    /// of the system's environment as it is now goes over what the next command would get, a `set`
    /// before it included; none is removed.
    pub fn reload(&mut self, fresh: Vec<(String, String)>) {
        for (name, value) in fresh {
            if self.get(&name).as_deref() != Some(value.as_str()) {
                self.store(&name, Some(value));
            }
        }
    }

    /// The changes in the order given: a name with its value, or `None` for a removed variable.
    pub fn changes(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.changes
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_deref()))
    }
}

/// Whether two variable names are one variable (`Path` and `PATH` on Windows).
fn same_name(a: &str, b: &str) -> bool {
    if process::VARIABLE_NAMES_DIFFER_BY_CASE {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_folders_come_back_last_first_and_show_in_the_variable() {
        let mut environment = Environment::default();
        environment.push_dir(r"C:\a%PATH%");
        environment.push_dir(r"C:\b");
        assert_eq!(
            environment.get(DIR_STACK_VARIABLE).as_deref(),
            Some(r"C:\b")
        );
        assert_eq!(environment.pop_dir().as_deref(), Some(r"C:\b"));
        assert_eq!(
            environment.get(DIR_STACK_VARIABLE).as_deref(),
            Some(r"C:\a%PATH%"),
            "taken literally"
        );
        assert_eq!(environment.pop_dir().as_deref(), Some(r"C:\a%PATH%"));
        assert_eq!(environment.get(DIR_STACK_VARIABLE), None);
        assert_eq!(environment.pop_dir(), None);
        environment.push_dir(r"C:\c");
        environment.clear_dirs();
        assert_eq!(environment.pop_dir(), None);
        assert_eq!(environment.get(DIR_STACK_VARIABLE), None);
    }

    #[test]
    fn a_title_is_taken_once() {
        let mut environment = Environment::default();
        environment.set_title("t".to_string());
        assert_eq!(environment.take_title().as_deref(), Some("t"));
        assert_eq!(environment.take_title(), None);
    }

    #[cfg(windows)]
    #[test]
    fn set_replaces_and_removes_without_regard_to_case() {
        let mut environment = Environment::default();
        environment.set("NC_TEST_A", "one");
        environment.set("nc_test_a", "two");
        assert_eq!(environment.get("Nc_Test_A").as_deref(), Some("two"));
        assert_eq!(environment.changes().count(), 1);
        environment.set("NC_TEST_A", "");
        assert_eq!(environment.get("NC_TEST_A"), None);
        assert_eq!(
            environment.changes().collect::<Vec<_>>(),
            [("NC_TEST_A", None)]
        );
    }

    #[cfg(windows)]
    #[test]
    fn expands_known_variables_and_keeps_the_rest() {
        let mut environment = Environment::default();
        environment.set("NC_TEST_B", "bee");
        assert_eq!(environment.expand("x%NC_TEST_B%y"), "xbeey");
        assert_eq!(
            environment.expand("%NC_TEST_UNSET_9% 100% %%"),
            "%NC_TEST_UNSET_9% 100% %%"
        );
        environment.set("NC_TEST_C", "%NC_TEST_B%;more");
        assert_eq!(environment.get("NC_TEST_C").as_deref(), Some("bee;more"));
    }

    #[test]
    fn a_reload_puts_the_systems_values_over_and_removes_nothing() {
        let mut environment = Environment::default();
        environment.set("NC_TEST_R1", "mine");
        environment.set("NC_TEST_R2", "kept");
        let path = std::env::var("PATH").unwrap_or_default();
        environment.reload(vec![
            ("NC_TEST_R1".to_string(), "system".to_string()),
            ("NC_TEST_R3".to_string(), "new".to_string()),
            ("PATH".to_string(), path.clone()),
        ]);
        assert_eq!(environment.get("NC_TEST_R1").as_deref(), Some("system"));
        assert_eq!(environment.get("NC_TEST_R2").as_deref(), Some("kept"));
        assert_eq!(environment.get("NC_TEST_R3").as_deref(), Some("new"));
        let names: Vec<&str> = environment.changes().map(|(name, _)| name).collect();
        assert!(!names.contains(&"PATH"), "an unchanged value is no change");
    }

    #[test]
    fn the_process_environment_shows_through() {
        let environment = Environment::default();
        assert_eq!(environment.get("PATH"), std::env::var("PATH").ok());
    }
}
