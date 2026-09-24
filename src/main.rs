mod core;
mod i18n;
mod linux_backend;

use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    thread,
    time::Duration,
};

const APP_ID: &str = "io.github.aircard.AirCard";

#[derive(Default)]
struct State {
    device: Option<core::Device>,
    image: Option<PathBuf>,
    theme: Option<PathBuf>,
    card_hash: String,
}

#[derive(Clone)]
struct UiTexts {
    labels: Vec<(gtk::Label, &'static str)>,
    buttons: Vec<(gtk::Button, &'static str)>,
    expanders: Vec<(gtk::Expander, &'static str)>,
    hash_entry: gtk::Entry,
    settings_button: gtk::MenuButton,
    device_label: gtk::Label,
    image_label: gtk::Label,
    theme_label: gtk::Label,
    tools_label: gtk::Label,
}

impl UiTexts {
    fn update(&self, language: i18n::Language, state: &State) {
        for (label, key) in &self.labels {
            label.set_text(i18n::tr(language, key));
        }
        for (button, key) in &self.buttons {
            button.set_label(i18n::tr(language, key));
        }
        for (expander, key) in &self.expanders {
            expander.set_label(Some(i18n::tr(language, key)));
        }
        self.hash_entry
            .set_placeholder_text(Some(i18n::tr(language, "wallet.hash_placeholder")));
        self.settings_button
            .set_label(i18n::tr(language, "settings.tooltip"));
        self.settings_button
            .set_tooltip_text(Some(i18n::tr(language, "settings.tooltip")));
        if state.device.is_none() {
            self.device_label
                .set_text(i18n::tr(language, "device.none"));
        }
        if state.image.is_none() {
            self.image_label
                .set_text(i18n::tr(language, "wallet.image_none"));
        }
        if state.theme.is_none() {
            self.theme_label.set_text(i18n::tr(language, "theme.none"));
        }
        if core::tools_status() == "libimobiledevice is available" {
            self.tools_label
                .set_text(i18n::tr(language, "device.info_available"));
        }
    }
}

fn toast(overlay: &adw::ToastOverlay, msg: impl AsRef<str>) {
    overlay.add_toast(adw::Toast::new(msg.as_ref()));
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--list-devices") {
        return match core::list_devices() {
            Ok(devices) => {
                for device in devices {
                    println!(
                        "{}\t{}\t{}\tiOS {}",
                        device.udid, device.name, device.product, device.version
                    );
                }
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e:#}");
                glib::ExitCode::FAILURE
            }
        };
    }
    if args.get(1).is_some_and(|arg| arg == "--probe-device") {
        let result = if args.len() == 3 {
            linux_backend::probe_device(&args[2])
        } else {
            Err(anyhow::anyhow!("Usage: aircard --probe-device <udid>"))
        };
        return match result {
            Ok(()) => {
                println!("Pairing handshake and AFC connection succeeded.");
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e:#}");
                glib::ExitCode::FAILURE
            }
        };
    }
    if args.get(1).is_some_and(|arg| arg == "--restore-books") {
        let result = if args.len() == 4 {
            linux_backend::restore_backup(&args[2], std::path::Path::new(&args[3]))
        } else {
            Err(anyhow::anyhow!(
                "Usage: aircard --restore-books <udid> <backup-directory>"
            ))
        };
        return match result {
            Ok(()) => {
                println!("Books state restored.");
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e:#}");
                glib::ExitCode::FAILURE
            }
        };
    }
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let state = Rc::new(RefCell::new(State::default()));
    let language = Rc::new(Cell::new(i18n::load_language()));
    let overlay = adw::ToastOverlay::new();
    let toolbar = adw::ToolbarView::new();
    let title = gtk::Label::new(Some("AirCard"));
    title.add_css_class("title-3");
    let header = adw::HeaderBar::builder().title_widget(&title).build();
    let settings_button = gtk::MenuButton::new();
    let settings_popover = gtk::Popover::new();
    let settings_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    settings_box.set_margin_top(12);
    settings_box.set_margin_bottom(12);
    settings_box.set_margin_start(16);
    settings_box.set_margin_end(16);
    let language_heading = gtk::Label::new(None);
    language_heading.set_xalign(0.0);
    settings_box.append(&language_heading);
    let english_choice = gtk::CheckButton::with_label("English");
    let korean_choice = gtk::CheckButton::with_label("한국어");
    korean_choice.set_group(Some(&english_choice));
    let japanese_choice = gtk::CheckButton::with_label("日本語");
    japanese_choice.set_group(Some(&english_choice));
    for choice in [&english_choice, &korean_choice, &japanese_choice] {
        settings_box.append(choice);
    }
    match language.get() {
        i18n::Language::English => english_choice.set_active(true),
        i18n::Language::Korean => korean_choice.set_active(true),
        i18n::Language::Japanese => japanese_choice.set_active(true),
    }
    settings_popover.set_child(Some(&settings_box));
    settings_button.set_popover(Some(&settings_popover));
    header.pack_end(&settings_button);
    toolbar.add_top_bar(&header);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
    content.set_margin_top(28);
    content.set_margin_bottom(28);
    content.set_margin_start(36);
    content.set_margin_end(36);
    scroll.set_child(Some(&content));
    toolbar.set_content(Some(&scroll));
    overlay.set_child(Some(&toolbar));

    let device_heading = section_heading("title-2");
    let device_label = gtk::Label::new(None);
    device_label.set_xalign(0.0);
    device_label.set_hexpand(true);
    device_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let refresh = gtk::Button::new();
    let status = core::tools_status();
    let tools_label = gtk::Label::new(Some(&status));
    tools_label.set_xalign(0.0);
    tools_label.add_css_class("dim-label");
    let device_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    device_row.append(&device_heading);
    device_row.append(&device_label);
    device_row.append(&refresh);
    let device_section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    device_section.append(&device_row);
    device_section.append(&tools_label);
    content.append(&device_section);

    let wallet_heading = section_heading("title-2");
    content.append(&wallet_heading);
    let wallet_frame = gtk::Frame::new(None);
    let wallet_body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    wallet_body.set_margin_top(18);
    wallet_body.set_margin_bottom(18);
    wallet_body.set_margin_start(18);
    wallet_body.set_margin_end(18);
    let card_step = section_heading("title-4");
    wallet_body.append(&card_step);
    let hash = gtk::Entry::new();
    hash.set_hexpand(true);
    let scan = gtk::Button::new();
    let hash_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    hash_row.append(&scan);
    hash_row.append(&hash);
    wallet_body.append(&hash_row);
    let image_step = section_heading("title-4");
    image_step.set_margin_top(8);
    wallet_body.append(&image_step);
    let image_label = gtk::Label::new(None);
    image_label.set_xalign(0.0);
    image_label.set_hexpand(true);
    image_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let choose_image = gtk::Button::new();
    let image_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    image_row.append(&choose_image);
    image_row.append(&image_label);
    wallet_body.append(&image_row);
    let apply_skin = gtk::Button::new();
    apply_skin.add_css_class("suggested-action");
    apply_skin.set_halign(gtk::Align::End);
    wallet_body.append(&apply_skin);
    let wallet_info = gtk::Label::new(None);
    wallet_info.set_xalign(0.0);
    wallet_info.set_wrap(true);
    wallet_info.add_css_class("dim-label");
    wallet_body.append(&wallet_info);
    wallet_frame.set_child(Some(&wallet_body));
    content.append(&wallet_frame);

    let theme_expander = gtk::Expander::new(None);
    let theme_label = gtk::Label::new(None);
    theme_label.set_xalign(0.0);
    theme_label.set_wrap(true);
    let choose_theme = gtk::Button::new();
    let apply_theme = gtk::Button::new();
    apply_theme.add_css_class("suggested-action");
    let theme_info = gtk::Label::new(None);
    theme_info.set_xalign(0.0);
    theme_info.set_wrap(true);
    theme_info.add_css_class("dim-label");
    let theme_body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    theme_body.set_margin_top(12);
    theme_body.set_margin_start(18);
    theme_body.append(&theme_label);
    theme_body.append(&choose_theme);
    theme_body.append(&apply_theme);
    theme_body.append(&theme_info);
    theme_expander.set_child(Some(&theme_body));
    content.append(&theme_expander);

    let activity_expander = gtk::Expander::new(None);
    let log_view = gtk::TextView::new();
    log_view.set_editable(false);
    log_view.set_monospace(true);
    log_view
        .buffer()
        .set_text(&format!("{}\n", i18n::tr(language.get(), "activity.ready")));
    let log_scroll = gtk::ScrolledWindow::new();
    log_scroll.set_size_request(-1, 180);
    log_scroll.set_child(Some(&log_view));
    let activity_body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    activity_body.set_margin_top(12);
    activity_body.set_margin_start(18);
    activity_body.append(&log_scroll);
    activity_expander.set_child(Some(&activity_body));
    content.append(&activity_expander);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("AirCard")
        .default_width(900)
        .default_height(620)
        .content(&overlay)
        .build();
    let texts = UiTexts {
        labels: vec![
            (language_heading, "settings.language"),
            (device_heading, "device.title"),
            (wallet_heading, "wallet.title"),
            (card_step, "wallet.step_card"),
            (image_step, "wallet.step_image"),
            (wallet_info, "wallet.info"),
            (theme_info, "theme.info"),
        ],
        buttons: vec![
            (refresh.clone(), "device.refresh"),
            (choose_image.clone(), "wallet.choose_image"),
            (apply_skin.clone(), "wallet.apply"),
            (choose_theme.clone(), "theme.choose"),
            (apply_theme.clone(), "theme.apply"),
            (scan.clone(), "activity.scan"),
        ],
        expanders: vec![
            (theme_expander.clone(), "theme.title"),
            (activity_expander.clone(), "activity.title"),
        ],
        hash_entry: hash.clone(),
        settings_button,
        device_label: device_label.clone(),
        image_label: image_label.clone(),
        theme_label: theme_label.clone(),
        tools_label,
    };
    texts.update(language.get(), &state.borrow());
    for (choice, selected) in [
        (english_choice, i18n::Language::English),
        (korean_choice, i18n::Language::Korean),
        (japanese_choice, i18n::Language::Japanese),
    ] {
        let language = language.clone();
        let texts = texts.clone();
        let state = state.clone();
        let overlay = overlay.clone();
        let popover = settings_popover.clone();
        choice.connect_toggled(move |choice| {
            if !choice.is_active() {
                return;
            }
            language.set(selected);
            texts.update(selected, &state.borrow());
            if let Err(error) = i18n::save_language(selected) {
                toast(
                    &overlay,
                    i18n::format(
                        selected,
                        "settings.save_failed",
                        &[("error", &error.to_string())],
                    ),
                );
            }
            popover.popdown();
        });
    }
    {
        let state = state.clone();
        let overlay = overlay.clone();
        let label = device_label.clone();
        let language = language.clone();
        refresh.connect_clicked(move |_| match core::list_devices() {
            Ok(devices) if devices.is_empty() => {
                toast(&overlay, i18n::tr(language.get(), "device.not_found"))
            }
            Ok(mut devices) => {
                let d = devices.remove(0);
                label.set_text(&format!("{} — {} (iOS {})", d.name, d.product, d.version));
                state.borrow_mut().device = Some(d);
                toast(&overlay, i18n::tr(language.get(), "device.selected"));
            }
            Err(e) => toast(&overlay, e.to_string()),
        });
    }
    choose_file(
        &choose_image,
        "wallet.choose_image",
        &overlay,
        state.clone(),
        language.clone(),
        image_label.clone(),
        true,
    );
    choose_file(
        &choose_theme,
        "theme.choose",
        &overlay,
        state.clone(),
        language.clone(),
        theme_label.clone(),
        false,
    );
    {
        let state = state.clone();
        let overlay = overlay.clone();
        let hash = hash.clone();
        let button = apply_skin.clone();
        let log = log_view.buffer();
        let language = language.clone();
        let activity_expander = activity_expander.clone();
        apply_skin.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            s.card_hash = hash.text().to_string();
            let args = s
                .device
                .as_ref()
                .zip(s.image.as_ref())
                .map(|(d, p)| (d.udid.clone(), p.clone(), s.card_hash.clone()));
            drop(s);
            let Some((udid, path, card_hash)) = args else {
                toast(
                    &overlay,
                    i18n::tr(language.get(), "wallet.need_device_image"),
                );
                return;
            };
            if card_hash.is_empty() {
                toast(&overlay, i18n::tr(language.get(), "wallet.need_hash"));
                return;
            }
            activity_expander.set_expanded(true);
            run_task(
                &button,
                &overlay,
                &log,
                language.clone(),
                "wallet.applying",
                move || {
                    let skin = core::prepare_skin(&path)?;
                    let result = core::stage_apply("skin", &udid, &skin, Some(&card_hash));
                    let _ = std::fs::remove_file(&skin);
                    result
                },
            );
        });
    }
    {
        let state = state.clone();
        let overlay = overlay.clone();
        let button = apply_theme.clone();
        let log = log_view.buffer();
        let language = language.clone();
        let activity_expander = activity_expander.clone();
        apply_theme.connect_clicked(move |_| {
            let s = state.borrow();
            let args = s
                .device
                .as_ref()
                .zip(s.theme.as_ref())
                .map(|(d, p)| (d.udid.clone(), p.clone()));
            drop(s);
            let Some((udid, path)) = args else {
                toast(&overlay, i18n::tr(language.get(), "theme.need_device_file"));
                return;
            };
            activity_expander.set_expanded(true);
            run_task(
                &button,
                &overlay,
                &log,
                language.clone(),
                "theme.applying",
                move || core::stage_apply("theme", &udid, &path, None),
            );
        });
    }
    {
        let overlay = overlay.clone();
        let buffer = log_view.buffer();
        let state = state.clone();
        let scan_button = scan.clone();
        let hash_entry = hash.clone();
        let language = language.clone();
        let activity_expander = activity_expander.clone();
        scan.connect_clicked(move |_| {
            let Some(device) = state.borrow().device.clone() else {
                toast(&overlay, i18n::tr(language.get(), "activity.need_device"));
                return;
            };
            activity_expander.set_expanded(true);
            scan_button.set_sensitive(false);
            buffer.insert_at_cursor(&format!(
                "{}\n",
                i18n::tr(language.get(), "activity.scanning")
            ));
            let (tx, rx) = mpsc::channel::<Option<String>>();
            thread::spawn(move || {
                let child = std::process::Command::new("timeout")
                    .args(["60s", "idevicesyslog", "--no-colors", "-u", &device.udid])
                    .stdout(std::process::Stdio::piped())
                    .spawn();
                if let Ok(mut child) = child {
                    if let Some(stdout) = child.stdout.take() {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            for found in core::extract_card_hashes(&line) {
                                let _ = tx.send(Some(found));
                            }
                        }
                    }
                    let _ = child.wait();
                }
                let _ = tx.send(None);
            });
            let button = scan_button.clone();
            let buffer = buffer.clone();
            let hash = hash_entry.clone();
            let overlay = overlay.clone();
            let language = language.clone();
            glib::timeout_add_local(Duration::from_millis(200), move || {
                while let Ok(item) = rx.try_recv() {
                    match item {
                        Some(value) => {
                            hash.set_text(&value);
                            buffer.insert_at_cursor(&format!(
                                "{}\n",
                                i18n::format(
                                    language.get(),
                                    "activity.hash_found",
                                    &[("hash", &value)]
                                )
                            ));
                            toast(&overlay, i18n::tr(language.get(), "activity.card_detected"));
                        }
                        None => {
                            button.set_sensitive(true);
                            buffer.insert_at_cursor(&format!(
                                "{}\n",
                                i18n::tr(language.get(), "activity.scan_ended")
                            ));
                            return glib::ControlFlow::Break;
                        }
                    }
                }
                glib::ControlFlow::Continue
            });
        });
    }
    window.present();
}

fn run_task(
    button: &gtk::Button,
    overlay: &adw::ToastOverlay,
    log: &gtk::TextBuffer,
    language: Rc<Cell<i18n::Language>>,
    label_key: &'static str,
    job: impl FnOnce() -> anyhow::Result<String> + Send + 'static,
) {
    button.set_sensitive(false);
    log.insert_at_cursor(&format!("{}...\n", i18n::tr(language.get(), label_key)));
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(job().map_err(|e| format!("{e:#}")));
    });
    let button = button.clone();
    let overlay = overlay.clone();
    let log = log.clone();
    glib::timeout_add_local(Duration::from_millis(200), move || match rx.try_recv() {
        Ok(result) => {
            button.set_sensitive(true);
            let message = match result {
                Ok(value) => localize_success(language.get(), &value),
                Err(error) => i18n::format(language.get(), "task.failed", &[("error", &error)]),
            };
            log.insert_at_cursor(&format!("{message}\n"));
            toast(&overlay, &message);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            button.set_sensitive(true);
            toast(&overlay, i18n::tr(language.get(), "task.worker_stopped"));
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
    });
}

fn localize_success(language: i18n::Language, message: &str) -> String {
    if message == "Theme sent. Restart the iPhone to inspect the result." {
        return i18n::tr(language, "theme.success").to_owned();
    }
    if let Some(rest) = message.strip_prefix("Artwork sent; ")
        && let Some(count) = rest.split_whitespace().next()
        && count.parse::<usize>().is_ok()
    {
        return i18n::format(language, "wallet.success", &[("count", count)]);
    }
    message.to_owned()
}

fn section_heading(css_class: &str) -> gtk::Label {
    let heading = gtk::Label::new(None);
    heading.add_css_class(css_class);
    heading.set_xalign(0.0);
    heading
}

fn choose_file(
    button: &gtk::Button,
    title_key: &'static str,
    overlay: &adw::ToastOverlay,
    state: Rc<RefCell<State>>,
    language: Rc<Cell<i18n::Language>>,
    label: gtk::Label,
    image: bool,
) {
    let overlay = overlay.clone();
    button.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder()
            .title(i18n::tr(language.get(), title_key))
            .build();
        let overlay = overlay.clone();
        let state = state.clone();
        let label = label.clone();
        let language = language.clone();
        dialog.open(None::<&gtk::Window>, gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                match file.path() {
                    Some(path) => {
                        if image {
                            state.borrow_mut().image = Some(path.clone());
                        } else {
                            match core::theme_summary(&path) {
                                Ok(count) => {
                                    state.borrow_mut().theme = Some(path.clone());
                                    toast(
                                        &overlay,
                                        i18n::format(
                                            language.get(),
                                            "theme.summary",
                                            &[("count", &count.to_string())],
                                        ),
                                    );
                                }
                                Err(e) => {
                                    toast(&overlay, e.to_string());
                                    return;
                                }
                            }
                        }
                        label.set_text(&path.display().to_string());
                    }
                    None => toast(&overlay, i18n::tr(language.get(), "file.no_local_path")),
                }
            }
        });
    });
}
