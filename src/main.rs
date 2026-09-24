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
const PAGE_TITLES: [&str; 5] = [
    "device.title",
    "wallet.title",
    "theme.title",
    "activity.title",
    "settings.tooltip",
];

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
    page_title: gtk::Label,
    content_page: adw::NavigationPage,
    selected_page: Rc<Cell<usize>>,
    hash_entry: gtk::Entry,
    device_label: gtk::Label,
    status_label: gtk::Label,
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
        let index = self.selected_page.get();
        let title = i18n::tr(language, PAGE_TITLES[index]);
        self.page_title.set_text(title);
        self.content_page.set_title(title);
        self.hash_entry
            .set_placeholder_text(Some(i18n::tr(language, "wallet.hash_placeholder")));
        if state.device.is_none() {
            self.device_label
                .set_text(i18n::tr(language, "device.none"));
            self.status_label
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
                .set_text(i18n::tr(language, "settings.dependency_available"));
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
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    install_styles();
    let overlay = adw::ToastOverlay::new();
    let split = adw::NavigationSplitView::new();
    split.set_sidebar_width_unit(adw::LengthUnit::Px);
    split.set_min_sidebar_width(184.0);
    split.set_max_sidebar_width(184.0);
    split.set_sidebar_width_fraction(0.2);

    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_css_class("aircard-sidebar");
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.add_css_class("aircard-sidebar-header");
    let sidebar_title = gtk::Label::new(Some("AirCard"));
    sidebar_title.add_css_class("aircard-sidebar-title");
    sidebar_title.set_margin_start(8);
    sidebar_header.pack_start(&sidebar_title);
    sidebar_header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    sidebar_toolbar.add_top_bar(&sidebar_header);
    let sidebar_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_body.add_css_class("aircard-sidebar");
    let nav = gtk::ListBox::new();
    nav.add_css_class("aircard-nav");
    nav.set_selection_mode(gtk::SelectionMode::Single);
    nav.set_activate_on_single_click(true);
    let mut nav_labels = Vec::new();
    for key in [
        "device.tab",
        "wallet.tab",
        "theme.tab",
        "activity.tab",
        "settings.tooltip",
    ] {
        let (row, label) = sidebar_row();
        nav.append(&row);
        nav_labels.push((label, key));
    }
    let nav_scroll = gtk::ScrolledWindow::new();
    nav_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    nav_scroll.set_vexpand(true);
    nav_scroll.set_child(Some(&nav));
    sidebar_body.append(&nav_scroll);
    sidebar_body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let status_badge = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    status_badge.add_css_class("aircard-status");
    let status_label = gtk::Label::new(None);
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_xalign(0.0);
    status_badge.append(&status_label);
    sidebar_body.append(&status_badge);
    sidebar_toolbar.set_content(Some(&sidebar_body));
    let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, "AirCard");
    split.set_sidebar(Some(&sidebar_page));

    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.add_css_class("aircard-content");
    let content_header = adw::HeaderBar::new();
    content_header.add_css_class("aircard-content-header");
    let page_title = gtk::Label::new(None);
    page_title.add_css_class("title-4");
    page_title.set_margin_start(12);
    page_title.set_xalign(0.0);
    content_header.pack_start(&page_title);
    content_header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let settings_body = page_body();
    let settings_box = card_box();
    let language_heading = gtk::Label::new(None);
    language_heading.set_xalign(0.0);
    settings_box.append(&language_heading);
    let english_choice = gtk::CheckButton::with_label("English");
    let korean_choice = gtk::CheckButton::with_label("한국어");
    korean_choice.set_group(Some(&english_choice));
    let japanese_choice = gtk::CheckButton::with_label("日本語");
    japanese_choice.set_group(Some(&english_choice));
    let language_choices = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    for choice in [&korean_choice, &english_choice, &japanese_choice] {
        language_choices.append(choice);
    }
    settings_box.append(&language_choices);
    settings_body.append(&settings_box);
    let dependencies_card = card_box();
    let dependencies_heading = gtk::Label::new(None);
    dependencies_heading.set_xalign(0.0);
    dependencies_card.append(&dependencies_heading);
    let dependency_name = gtk::Label::new(Some("libimobiledevice"));
    dependency_name.set_xalign(0.0);
    let status = core::tools_status();
    let tools_label = gtk::Label::new(Some(&status));
    tools_label.set_xalign(1.0);
    tools_label.set_hexpand(true);
    tools_label.set_wrap(true);
    tools_label.add_css_class("aircard-muted");
    let dependency_row = card_row(&dependency_name, &tools_label);
    dependencies_card.append(&dependency_row);
    settings_body.append(&dependencies_card);
    match language.get() {
        i18n::Language::English => english_choice.set_active(true),
        i18n::Language::Korean => korean_choice.set_active(true),
        i18n::Language::Japanese => japanese_choice.set_active(true),
    }
    content_toolbar.add_top_bar(&content_header);
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(150);
    stack.add_named(&page_scroller(&settings_body), Some("settings"));
    content_toolbar.set_content(Some(&stack));
    let content_page = adw::NavigationPage::new(&content_toolbar, "Device");
    split.set_content(Some(&content_page));
    split.set_show_content(true);
    overlay.set_child(Some(&split));

    let device_body = page_body();
    let device_card = card_box();
    let device_heading = section_heading();
    device_card.append(&device_heading);
    let device_label = gtk::Label::new(None);
    device_label.set_xalign(0.0);
    device_label.set_hexpand(true);
    device_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let refresh = gtk::Button::new();
    let device_row = card_row(&device_label, &refresh);
    device_card.append(&device_row);
    device_body.append(&device_card);
    stack.add_named(&page_scroller(&device_body), Some("device"));

    let wallet_body = page_body();
    let hash_card = card_box();
    let card_step = section_heading();
    hash_card.append(&card_step);
    let hash = gtk::Entry::new();
    hash.set_hexpand(true);
    let scan = gtk::Button::new();
    let hash_row = card_row(&hash, &scan);
    hash_card.append(&hash_row);
    wallet_body.append(&hash_card);
    let image_card = card_box();
    let image_step = section_heading();
    image_card.append(&image_step);
    let image_label = gtk::Label::new(None);
    image_label.set_xalign(0.0);
    image_label.set_hexpand(true);
    image_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let choose_image = gtk::Button::new();
    let image_row = card_row(&image_label, &choose_image);
    image_card.append(&image_row);
    image_card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let apply_skin = gtk::Button::new();
    apply_skin.add_css_class("aircard-primary");
    apply_skin.set_halign(gtk::Align::End);
    image_card.append(&apply_skin);
    let wallet_info = gtk::Label::new(None);
    wallet_info.set_xalign(0.0);
    wallet_info.set_wrap(true);
    wallet_info.add_css_class("aircard-muted");
    image_card.append(&wallet_info);
    wallet_body.append(&image_card);
    stack.add_named(&page_scroller(&wallet_body), Some("wallet"));

    let theme_body = page_body();
    let theme_card = card_box();
    let theme_heading = section_heading();
    theme_card.append(&theme_heading);
    let theme_label = gtk::Label::new(None);
    theme_label.set_xalign(0.0);
    theme_label.set_wrap(true);
    theme_label.set_hexpand(true);
    let choose_theme = gtk::Button::new();
    let theme_row = card_row(&theme_label, &choose_theme);
    theme_card.append(&theme_row);
    theme_card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let apply_theme = gtk::Button::new();
    apply_theme.add_css_class("aircard-primary");
    apply_theme.set_halign(gtk::Align::End);
    theme_card.append(&apply_theme);
    let theme_info = gtk::Label::new(None);
    theme_info.set_xalign(0.0);
    theme_info.set_wrap(true);
    theme_info.add_css_class("aircard-muted");
    theme_card.append(&theme_info);
    theme_body.append(&theme_card);
    stack.add_named(&page_scroller(&theme_body), Some("theme"));

    let activity_body = page_body();
    activity_body.set_vexpand(true);
    let activity_stack = gtk::Stack::new();
    activity_stack.set_vexpand(true);
    let empty_title = gtk::Label::new(None);
    empty_title.add_css_class("aircard-muted");
    empty_title.set_wrap(true);
    empty_title.set_justify(gtk::Justification::Center);
    empty_title.set_halign(gtk::Align::Center);
    empty_title.set_valign(gtk::Align::Center);
    activity_stack.add_named(&empty_title, Some("empty"));
    let log_list = gtk::ListBox::new();
    log_list.add_css_class("aircard-log-list");
    log_list.set_selection_mode(gtk::SelectionMode::None);
    log_list.set_show_separators(true);
    activity_stack.add_named(&log_list, Some("log"));
    activity_stack.set_visible_child_name("empty");
    activity_body.append(&activity_stack);
    stack.add_named(&page_scroller(&activity_body), Some("activity"));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("AirCard")
        .default_width(960)
        .default_height(640)
        .content(&overlay)
        .build();
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Px,
    ));
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    for row in [&device_row, &hash_row, &image_row, &theme_row] {
        narrow.add_setter(
            row,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
    }
    window.add_breakpoint(narrow);
    let selected_page = Rc::new(Cell::new(0_usize));
    let texts = UiTexts {
        labels: nav_labels
            .into_iter()
            .chain([
                (language_heading, "settings.language"),
                (dependencies_heading, "settings.dependencies"),
                (device_heading, "device.title"),
                (card_step, "wallet.step_card"),
                (image_step, "wallet.step_image"),
                (wallet_info, "wallet.info"),
                (theme_heading, "theme.title"),
                (theme_info, "theme.info"),
                (empty_title, "activity.ready"),
            ])
            .collect(),
        buttons: vec![
            (refresh.clone(), "device.refresh"),
            (choose_image.clone(), "wallet.choose_image"),
            (apply_skin.clone(), "wallet.apply"),
            (choose_theme.clone(), "theme.choose"),
            (apply_theme.clone(), "theme.apply"),
            (scan.clone(), "activity.scan"),
        ],
        page_title: page_title.clone(),
        content_page: content_page.clone(),
        selected_page: selected_page.clone(),
        hash_entry: hash.clone(),
        device_label: device_label.clone(),
        status_label: status_label.clone(),
        image_label: image_label.clone(),
        theme_label: theme_label.clone(),
        tools_label,
    };
    texts.update(language.get(), &state.borrow());
    {
        let stack = stack.clone();
        let split = split.clone();
        let texts = texts.clone();
        let language = language.clone();
        let state = state.clone();
        nav.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let index = row.index() as usize;
            let name = ["device", "wallet", "theme", "activity", "settings"][index];
            texts.selected_page.set(index);
            texts.update(language.get(), &state.borrow());
            stack.set_visible_child_name(name);
            split.set_show_content(true);
        });
    }
    {
        let split = split.clone();
        nav.connect_row_activated(move |_, _| split.set_show_content(true));
    }
    nav.select_row(nav.row_at_index(0).as_ref());
    for (choice, selected) in [
        (english_choice, i18n::Language::English),
        (korean_choice, i18n::Language::Korean),
        (japanese_choice, i18n::Language::Japanese),
    ] {
        let language = language.clone();
        let texts = texts.clone();
        let state = state.clone();
        let overlay = overlay.clone();
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
        });
    }
    {
        let state = state.clone();
        let overlay = overlay.clone();
        let label = device_label.clone();
        let status_label = status_label.clone();
        let language = language.clone();
        refresh.connect_clicked(move |_| match core::list_devices() {
            Ok(devices) if devices.is_empty() => {
                let mut current = state.borrow_mut();
                current.device = None;
                drop(current);
                label.set_text(i18n::tr(language.get(), "device.none"));
                status_label.set_text(i18n::tr(language.get(), "device.none"));
                toast(&overlay, i18n::tr(language.get(), "device.not_found"));
            }
            Ok(mut devices) => {
                let d = devices.remove(0);
                label.set_text(&format!("{} — {} (iOS {})", d.name, d.product, d.version));
                status_label.set_text(&d.name);
                let mut current = state.borrow_mut();
                current.device = Some(d);
                toast(&overlay, i18n::tr(language.get(), "device.selected"));
            }
            Err(e) => {
                let mut current = state.borrow_mut();
                current.device = None;
                drop(current);
                label.set_text(i18n::tr(language.get(), "device.none"));
                status_label.set_text(i18n::tr(language.get(), "device.none"));
                toast(&overlay, e.to_string());
            }
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
        let log = log_list.clone();
        let language = language.clone();
        let activity_stack = activity_stack.clone();
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
            activity_stack.set_visible_child_name("log");
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
        let log = log_list.clone();
        let language = language.clone();
        let activity_stack = activity_stack.clone();
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
            activity_stack.set_visible_child_name("log");
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
        let log = log_list.clone();
        let state = state.clone();
        let scan_button = scan.clone();
        let hash_entry = hash.clone();
        let language = language.clone();
        let activity_stack = activity_stack.clone();
        scan.connect_clicked(move |_| {
            let Some(device) = state.borrow().device.clone() else {
                toast(&overlay, i18n::tr(language.get(), "activity.need_device"));
                return;
            };
            activity_stack.set_visible_child_name("log");
            scan_button.set_sensitive(false);
            append_log(&log, i18n::tr(language.get(), "activity.scanning"));
            let (tx, rx) = mpsc::channel::<Option<String>>();
            thread::spawn(move || {
                let mut seen = std::collections::HashSet::new();
                let child = std::process::Command::new("timeout")
                    .args(["60s", "idevicesyslog", "--no-colors", "-u", &device.udid])
                    .stdout(std::process::Stdio::piped())
                    .spawn();
                if let Ok(mut child) = child {
                    if let Some(stdout) = child.stdout.take() {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            for found in core::extract_card_hashes(&line) {
                                if seen.insert(found.clone()) {
                                    let _ = tx.send(Some(found));
                                }
                            }
                        }
                    }
                    let _ = child.wait();
                }
                let _ = tx.send(None);
            });
            let button = scan_button.clone();
            let log = log.clone();
            let hash = hash_entry.clone();
            let overlay = overlay.clone();
            let language = language.clone();
            glib::timeout_add_local(Duration::from_millis(200), move || {
                while let Ok(item) = rx.try_recv() {
                    match item {
                        Some(value) => {
                            hash.set_text(&value);
                            append_log(
                                &log,
                                &i18n::format(
                                    language.get(),
                                    "activity.hash_found",
                                    &[("hash", &value)],
                                ),
                            );
                            toast(&overlay, i18n::tr(language.get(), "activity.card_detected"));
                        }
                        None => {
                            button.set_sensitive(true);
                            append_log(&log, i18n::tr(language.get(), "activity.scan_ended"));
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
    log: &gtk::ListBox,
    language: Rc<Cell<i18n::Language>>,
    label_key: &'static str,
    job: impl FnOnce() -> anyhow::Result<String> + Send + 'static,
) {
    button.set_sensitive(false);
    append_log(log, &format!("{}...", i18n::tr(language.get(), label_key)));
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
            append_log(&log, &message);
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

fn install_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../resources/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("A display is required"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn sidebar_row() -> (gtk::ListBoxRow, gtk::Label) {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("aircard-nav-row");
    let label = gtk::Label::new(None);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.set_child(Some(&label));
    (row, label)
}

fn page_body() -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 24);
    body.set_margin_top(30);
    body.set_margin_bottom(30);
    body.set_margin_start(28);
    body.set_margin_end(28);
    body
}

fn page_scroller(body: &gtk::Box) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(800);
    clamp.set_vexpand(true);
    clamp.set_child(Some(body));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&clamp));
    scroll
}

fn card_box() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 16);
    card.add_css_class("aircard-card");
    card
}

fn card_row(left: &impl IsA<gtk::Widget>, right: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row.append(left);
    row.append(right);
    row
}

fn append_log(list: &gtk::ListBox, text: &str) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_selectable(true);
    row.set_child(Some(&label));
    list.append(&row);
}

fn section_heading() -> gtk::Label {
    let heading = gtk::Label::new(None);
    heading.add_css_class("aircard-section-label");
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
