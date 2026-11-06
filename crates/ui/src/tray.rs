use std::time::{Duration, Instant};

use rust_i18n::t;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

/// Windows' default double-click time. A left click waits this long before
/// showing the menu, so a double-click can open the window instead.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

pub(crate) struct SystemTray {
    icon: TrayIcon,
    open_item: MenuItem,
    quit_item: MenuItem,
    locale: String,
    clicks: Clicks,
}

impl std::fmt::Debug for SystemTray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemTray")
            .field("locale", &self.locale)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            // The right button shows the menu at once; the left one waits
            // for a possible double-click (see `Clicks`).
            .with_menu_on_left_click(!cfg!(windows))
            .with_icon(icon)
            .build()?;

        Ok(Self {
            icon: tray_icon,
            open_item,
            quit_item,
            locale: crate::i18n::current(),
            clicks: Clicks::default(),
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

    pub(crate) fn take_actions(&mut self) -> Vec<TrayAction> {
        let now = Instant::now();
        let mut actions: Vec<TrayAction> = MenuEvent::receiver()
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
            .collect();
        for event in TrayIconEvent::receiver().try_iter() {
            let click = match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => Click::LeftUp,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => Click::Double,
                _ => continue,
            };
            if let Some(action) = self.clicks.on(click, now) {
                actions.push(action);
            }
        }
        if self.clicks.menu_due(now) {
            self.icon.show_menu();
        }
        actions
    }

    /// When the app should look again, while a left click waits for a
    /// possible second one.
    pub(crate) fn wake_in(&self) -> Option<Duration> {
        self.clicks.wake_in(Instant::now())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    LeftUp,
    Double,
}

/// Tells a single left click (menu) from a double-click (open the window).
#[derive(Debug, Default)]
struct Clicks {
    /// When the menu of a single click is due.
    menu_at: Option<Instant>,
    /// The button release that ends a double-click is not a new click.
    ignore_until: Option<Instant>,
}

impl Clicks {
    fn on(&mut self, click: Click, now: Instant) -> Option<TrayAction> {
        match click {
            Click::Double => {
                self.menu_at = None;
                self.ignore_until = Some(now + DOUBLE_CLICK);
                Some(TrayAction::Open)
            }
            Click::LeftUp => {
                if !cfg!(windows) || self.ignore_until.is_some_and(|until| now < until) {
                    self.ignore_until = None;
                    return None;
                }
                self.menu_at = Some(now + DOUBLE_CLICK);
                None
            }
        }
    }

    fn menu_due(&mut self, now: Instant) -> bool {
        if self.menu_at.is_some_and(|at| now >= at) {
            self.menu_at = None;
            true
        } else {
            false
        }
    }

    fn wake_in(&self, now: Instant) -> Option<Duration> {
        self.menu_at.map(|at| at.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn a_single_left_click_shows_the_menu_after_the_double_click_time() {
        let mut clicks = Clicks::default();
        let start = Instant::now();
        assert_eq!(clicks.on(Click::LeftUp, start), None);
        assert!(!clicks.menu_due(start + DOUBLE_CLICK / 2));
        assert!(clicks.wake_in(start).is_some());
        assert!(clicks.menu_due(start + DOUBLE_CLICK));
        assert!(!clicks.menu_due(start + DOUBLE_CLICK * 2), "only once");
    }

    #[test]
    fn a_double_click_opens_the_window_without_the_menu() {
        let mut clicks = Clicks::default();
        let start = Instant::now();
        let step = Duration::from_millis(100);
        assert_eq!(clicks.on(Click::LeftUp, start), None);
        assert_eq!(
            clicks.on(Click::Double, start + step),
            Some(TrayAction::Open)
        );
        // The release that ends the double-click.
        assert_eq!(clicks.on(Click::LeftUp, start + step * 2), None);
        assert!(!clicks.menu_due(start + DOUBLE_CLICK * 3));
        assert_eq!(clicks.wake_in(start), None);
    }
}
