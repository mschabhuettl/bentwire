//! Tray icon (B-TRAY-1): a StatusNotifierItem with a dbusmenu, through ksni. The item
//! runs on ksni's own thread and sends its input back to the event loop.

use bentwire_core::api::{MenuAction, MenuEntry, TrayInput, TrayScene};
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;

pub struct BwTray {
    tx: calloop::channel::Sender<TrayInput>,
    title: String,
    tooltip: String,
    menu: Vec<MenuEntry>,
    icon: Vec<ksni::Icon>,
}

/// dbusmenu marks mnemonics with `_` where bentwire's captions use `&` (B-MENU-2).
pub fn dbusmenu_label(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '&' if it.peek() == Some(&'&') => {
                it.next();
                out.push('&');
            }
            '&' => out.push('_'),
            '_' => out.push_str("__"),
            c => out.push(c),
        }
    }
    out
}

/// Straight RGBA to SNI's ARGB32 in network byte order.
pub fn sni_icon(w: u32, h: u32, rgba: &[u8]) -> ksni::Icon {
    let data = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    ksni::Icon {
        width: w as i32,
        height: h as i32,
        data,
    }
}

/// bentwire's own icon (used when the character has no `icon.png`): a pale yellow
/// speech balloon with a dark outline, drawn in code.
pub fn own_icon() -> ksni::Icon {
    let n = 32u32;
    let mut px = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((fx - 16.0) / 14.0).powi(2) + ((fy - 13.0) / 10.0).powi(2);
            let tail = (6.0..=14.0).contains(&fx)
                && (20.0..=30.0).contains(&fy)
                && fx - 6.0 <= (30.0 - fy) * 0.8;
            let c: Option<[u8; 4]> = if d <= 0.82 || (tail && d > 0.8) {
                Some([0xFF, 0xFF, 0xCC, 0xFF])
            } else if d <= 1.0 {
                Some([0x40, 0x40, 0x40, 0xFF])
            } else {
                None
            };
            if let Some(c) = c {
                let i = ((y * n + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
    }
    sni_icon(n, n, &px)
}

impl ksni::Tray for BwTray {
    fn id(&self) -> String {
        "bentwire".into()
    }
    fn title(&self) -> String {
        self.title.clone()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.tooltip.clone(),
            ..Default::default()
        }
    }
    fn activate(&mut self, x: i32, y: i32) {
        let _ = self.tx.send(TrayInput::Activate { x, y });
    }
    fn secondary_activate(&mut self, x: i32, y: i32) {
        let _ = self.tx.send(TrayInput::SecondaryActivate { x, y });
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        self.menu
            .iter()
            .map(|e| match e {
                MenuEntry::Separator => ksni::MenuItem::Separator,
                MenuEntry::Item { label, action } => {
                    let action: MenuAction = *action;
                    StandardItem {
                        label: dbusmenu_label(&label.replace('\0', "")),
                        activate: Box::new(move |t: &mut BwTray| {
                            let _ = t.tx.send(TrayInput::Menu(action));
                        }),
                        ..Default::default()
                    }
                    .into()
                }
            })
            .collect()
    }
}

pub struct Tray {
    handle: Handle<BwTray>,
    last: Option<(TrayScene, bool)>,
}

impl Tray {
    /// Starts the item; None (logged) when there is no session bus or SNI host.
    pub fn spawn(tx: calloop::channel::Sender<TrayInput>) -> Option<Tray> {
        let t = BwTray {
            tx,
            title: String::new(),
            tooltip: String::new(),
            menu: Vec::new(),
            icon: vec![own_icon()],
        };
        // Plasma may start its tray host after bentwired.
        match t.assume_sni_available(true).spawn() {
            Ok(handle) => Some(Tray { handle, last: None }),
            Err(e) => {
                log::warn!("no tray icon: {e}");
                None
            }
        }
    }

    /// Mirror the core's tray model. `icon` is the character's icon.png, if any.
    pub fn update(&mut self, s: &TrayScene, icon: Option<ksni::Icon>) {
        let key = (s.clone(), icon.is_some());
        if self.last.as_ref() == Some(&key) {
            return;
        }
        self.last = Some(key);
        let s = s.clone();
        self.handle.update(move |t| {
            // D-Bus strings must not contain U+0000 (see dbus::safe).
            t.title = s.title.replace('\0', "");
            t.tooltip = s.tooltip.replace('\0', "");
            t.menu = s.menu;
            t.icon = vec![icon.unwrap_or_else(own_icon)];
        });
    }
}

/// Stop ksni's service before its handle goes: ksni 0.3.7 panics ("all futures in select!
/// were completed") when the handle is dropped after its bus connection has ended.
impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_tray_1_labels_and_icon() {
        assert_eq!(dbusmenu_label("&Hide"), "_Hide");
        assert_eq!(
            dbusmenu_label("Assistenten &wählen..."),
            "Assistenten _wählen..."
        );
        assert_eq!(dbusmenu_label("a_b && c"), "a__b & c");
        let i = sni_icon(1, 1, &[1, 2, 3, 4]);
        assert_eq!(i.data, vec![4, 1, 2, 3]);
        assert_eq!(own_icon().data.len(), 32 * 32 * 4);
    }
}
