//! nova-commander: a Windows console file manager in the Norton Commander tradition,
//! with an embedded terminal below the panels. Portable: settings live next to the executable.

mod app;
mod editor;
mod jobs;
mod platform;
mod plugin_host;
mod settings;
mod terminal;
mod ui;
mod vfs;

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use app::console_input::{Input, Reader};
use app::{App, Dialog};
use settings::Settings;

/// How long to wait for input before looking at the embedded terminal again.
const FRAME_INTERVAL: Duration = Duration::from_millis(30);

fn main() -> io::Result<()> {
    // One command line run on the user screen (ADR-0006): no UI, the command's exit code.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == terminal::runner::RUN_ARGUMENT)
    {
        std::process::exit(terminal::runner::run());
    }
    // The helper with the administrator's rights (ADR-0007): no UI, only its pipe.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == vfs::local::elevated::ELEVATED_ARGUMENT)
    {
        let arguments: Vec<String> = std::env::args_os()
            .skip(2)
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        std::process::exit(vfs::local::elevated::run_helper(&arguments));
    }
    if platform::process::reopen_in_plain_console() {
        return Ok(());
    }
    platform::process::survive_ctrl_break();
    let start_dir = std::env::current_dir()?;
    let settings_path = settings::default_path()?;
    let loaded = settings::load(&settings_path);
    // try_init enables raw mode and the alternate screen and installs a panic hook that restores
    // the console, so a crash never leaves the user's console unusable.
    ui::signals::remember_cursor_size();
    let mut terminal = ratatui::try_init()?;
    // Focus reports (`\x1b[?1004h`) let Windows Terminal tell us when another window is active, so the
    // modifiers held here are let go; classic conhost sends focus events anyway.
    ratatui::crossterm::execute!(io::stdout(), event::EnableFocusChange)?;
    let mut signals = ui::signals::Signals::new();
    let panics = settings::plugin_logs_dir(&settings_path).join("nova-commander.log");
    restore_console_on_panic(signals.original_title(), panics);
    let mouse = loaded.as_ref().map_or(true, |settings| settings.mouse);
    let result = platform::input::begin_console_input(mouse).and_then(|()| {
        run(
            &mut terminal,
            &start_dir,
            &settings_path,
            loaded,
            &mut signals,
        )
    });
    // Restoring the console matters more than switching focus reports off, so this goes on after a failure.
    // The console's own title comes back (a task may have shown its progress there).
    vfs::local::elevated_session::end();
    platform::input::end_console_input();
    let title_reset = signals.restore();
    let focus_reset = ratatui::crossterm::execute!(io::stdout(), event::DisableFocusChange);
    // The cursor's size is only its look: a console that does not take it is no failure.
    let _ = ui::signals::restore_cursor_size();
    ratatui::try_restore()?;
    focus_reset?;
    title_reset?;
    // The trees closed with the program finish saving their files.
    jobs::tree_worker::wait_for_saves(jobs::tree_worker::LAST_SAVES);
    let (settings, exit_code) = result?;
    if let Some(settings) = settings {
        settings::save(&settings_path, &settings)?;
    }
    // The code `exit` gave on the command line.
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// What the panic hook of `ratatui::try_init` does not put back; it still runs after this. Only for
/// the UI thread: a worker's panic is reported and the program goes on (`jobs::work`), so its text goes
/// to `log` and the console is left as it is (R7 review B1: ratatui's hook left the alternate screen
/// and raw mode for any thread's panic, and the text was printed over the screen).
fn restore_console_on_panic(original_title: Option<String>, log: PathBuf) {
    let screen = std::panic::take_hook();
    let ui_thread = std::thread::current().id();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() != ui_thread {
            log_worker_panic(&log, info);
            return;
        }
        platform::input::end_console_input();
        ui::signals::restore_after_panic(original_title.as_deref());
        screen(info);
    }));
}

/// A worker's panic appended to the program's log (nothing told when it cannot be written).
fn log_worker_panic(log: &Path, info: &std::panic::PanicHookInfo<'_>) {
    use std::io::Write;
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("a worker");
    let _ = log.parent().map(std::fs::create_dir_all);
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log);
    if let Ok(mut file) = opened {
        let _ = writeln!(file, "{name} panicked: {info}");
    }
}

/// What the program keeps beside its settings: the histories, the viewer's places, the themes, the
/// tree cache, the main user menu.
fn open_stores(app: &mut App, settings_path: &Path) {
    app.open_history(settings::history::history_path(settings_path));
    app.positions = Some(settings::positions::positions_path(settings_path));
    app.themes.built_in = ui::theme::built_in_names();
    app.themes.dir = settings::themes_dir(settings_path);
    app.tasks.tree_cache = Some(jobs::tree_cache::TreeCache::new(settings::tree_cache_dir(
        settings_path,
    )));
    app.set_main_menu_file(settings::user_menu::main_menu_path(settings_path));
}

/// Runs the UI loop. Returns the settings to save, or `None` when the settings file was broken (it is
/// then left untouched so the user's edits are not overwritten with defaults), and the exit code.
fn run(
    terminal: &mut DefaultTerminal,
    start_dir: &Path,
    settings_path: &Path,
    loaded: io::Result<Settings>,
    signals: &mut ui::signals::Signals,
) -> io::Result<(Option<Settings>, i32)> {
    let size = terminal.size()?;
    let (settings, load_error) = match loaded {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(error)),
    };
    let mut app = App::new(start_dir, size.width, size.height, &settings);
    open_stores(&mut app, settings_path);
    // `plugins` beside the program, as `settings` is (ADR-0004 §7).
    if let Some(root) = settings_path.parent().and_then(Path::parent) {
        let enabled = |name: &str| settings::plugin_enabled(&settings, name);
        app.set_plugins(plugin_host::PluginHost::load(
            &root.join("plugins"),
            &settings::plugin_logs_dir(settings_path),
            &settings.other,
            &enabled,
        ));
    }
    if let Some(error) = &load_error {
        app.show_dialog(Dialog::error(
            "Settings",
            vec![
                error.to_string(),
                "Defaults are used; the file will not be overwritten.".to_string(),
            ],
        ));
    }
    // The screen is drawn only when something changed: every draw re-shows the cursor, which restarts
    // the console's blink timer, so drawing on every frame made the cursor flicker.
    let mut dirty = true;
    let mut console_colors = ui::theme::console_colors::ConsoleColors::new(app.console_palette);
    let mut minute = current_minute();
    let mut focused = true;
    let mut input = Reader::default();
    let mut block_cursor = false;
    while !app.should_quit() {
        dirty |= app.tick();
        // A Shift+F9 held while work ran comes in the tick.
        if app.take_save_request() {
            save_setup(&mut app, settings_path, load_error.is_none());
        }
        signals.update(&mut app, focused)?;
        let now = current_minute();
        if now != minute {
            // The clock in the corner.
            minute = now;
            dirty = true;
        }
        // A viewer or an editor just opened is shown once its text is coloured (FAR's way: no plain
        // frame first); the screen before it stays meanwhile.
        if dirty && !app.holds_paint() {
            terminal.draw(|frame| {
                ui::draw(frame, &mut app);
                if app.console_palette {
                    ui::theme::console_palette().paint_exact(frame.buffer_mut());
                }
            })?;
            // After drawing: a draw is what takes up the theme chosen or moved over.
            console_colors.update()?;
            cursor_shape(&mut block_cursor, app.block_cursor())?;
            dirty = false;
        }
        // Everything that is queued, so fast typing never lags behind rendering.
        let mut wait = FRAME_INTERVAL;
        while let Some(event) = input.next(wait)? {
            wait = Duration::ZERO;
            dirty |= handle_input(&mut app, event, &mut focused);
            if app.take_save_request() {
                save_setup(&mut app, settings_path, load_error.is_none());
            }
            if app.should_quit() {
                break;
            }
        }
    }
    cursor_shape(&mut block_cursor, false)?;
    app.close_history();
    app.close_user_menu();
    app.close_extracted();
    let copies = app.plugin_copies();
    let ended = (
        load_error.is_none().then(|| app.to_settings()),
        app.exit_code(),
    );
    // The plugins end with the program's state; then the copies their panels were opened from go.
    drop(app);
    app::remove_copies_now(&copies);
    Ok(ended)
}

/// Shift+F9: writes the settings now. A settings file that failed to load is never overwritten.
fn save_setup(app: &mut App, path: &Path, loaded_cleanly: bool) {
    match settings::save_unless_broken(path, &app.to_settings(), loaded_cleanly) {
        Ok(true) => {}
        Ok(false) => app.show_dialog(Dialog::error(
            "Save setup",
            vec!["The settings file could not be read; it is left untouched.".to_string()],
        )),
        Err(error) => app.show_error("Save setup", &error),
    }
}

/// The console's cursor as the app wants it: FAR's big one for the editor's overtype, else the
/// user's own; sent only when it changes.
fn cursor_shape(shown: &mut bool, block: bool) -> io::Result<()> {
    use ratatui::crossterm::cursor::SetCursorStyle;
    if *shown == block {
        return Ok(());
    }
    *shown = block;
    let style = match block {
        true => SetCursorStyle::SteadyBlock,
        false => SetCursorStyle::DefaultUserShape,
    };
    ratatui::crossterm::execute!(io::stdout(), style)
}

fn current_minute() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 60)
}

/// Hands the input to the application; returns whether the screen may have changed.
fn handle_input(app: &mut App, input: Input, focused: &mut bool) -> bool {
    match input {
        Input::Key(key) => app.handle_key(key),
        // FAR's key bar follows Shift/Ctrl/Alt.
        Input::Held(held) => return app.set_held_modifiers(held),
        Input::Mouse(mouse) => return app.handle_mouse(mouse),
        Input::Resize(width, height) => app.resize(width, height),
        Input::Focus(now) => {
            *focused = now;
            app.focus_changed();
        }
    }
    true
}
