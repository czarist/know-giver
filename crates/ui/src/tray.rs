use rust_i18n::t;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

pub(crate) struct SystemTray {
    _icon: TrayIcon,
    open_item: MenuItem,
    quit_item: MenuItem,
    locale: String,
}

pub(crate) enum TrayAction {
    Open,
    Quit,
}

impl SystemTray {
    pub(crate) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let open_item = MenuItem::new(t!("tray.open"), true, None);
        let quit_item = MenuItem::new(t!("tray.quit"), true, None);
        let menu = Menu::with_items(&[&open_item, &quit_item])?;
        let icon = Icon::from_rgba(crate::icon::rgba(32), 32, 32)?;
        let tray_icon = TrayIconBuilder::new()
            .with_tooltip(t!("app.name"))
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()?;

        Ok(Self {
            _icon: tray_icon,
            open_item,
            quit_item,
            locale: crate::i18n::current(),
        })
    }

    pub(crate) fn set_locale(&mut self, locale: &str) {
        if self.locale == locale {
            return;
        }
        self.locale = locale.to_owned();
        self.open_item.set_text(t!("tray.open"));
        self.quit_item.set_text(t!("tray.quit"));
    }

    pub(crate) fn take_actions(&self) -> Vec<TrayAction> {
        MenuEvent::receiver()
            .try_iter()
            .filter_map(|event| {
                if event.id() == self.open_item.id() {
                    Some(TrayAction::Open)
                } else if event.id() == self.quit_item.id() {
                    Some(TrayAction::Quit)
                } else {
                    None
                }
            })
            .collect()
    }
}
