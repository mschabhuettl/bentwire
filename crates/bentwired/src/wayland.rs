//! Wayland backend (spec 3.11): one zwlr_layer_surface_v1 for the character (B-WL-2),
//! balloons as wl_subsurfaces (B-WL-8), the menu as an xdg_popup on the layer surface
//! (B-WL-9), wl_shm ARGB8888 premultiplied buffers with buffer scale or viewporter
//! (B-WL-6), input regions from the shape, drags through the implicit grab (B-WL-7).

use crate::app::Backend;
use crate::present::{Buf, Rendered};
use bentwire_core::api::{
    Event, Key, MenuInput, OutputInfo, PointerEvent as CoreEvent, PointerKind, Rect, Scene,
    Surface as CoreSurface,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_registry,
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        relative_pointer::{RelativeMotionEvent, RelativePointerHandler, RelativePointerState},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::{
            XdgPositioner, XdgShell,
            popup::{Popup, PopupConfigure, PopupHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
    subcompositor::SubcompositorState,
};
use std::os::fd::{AsFd, AsRawFd, RawFd};
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_subsurface, wl_surface},
};
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1;
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::xdg_positioner;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

pub struct Wayland {
    conn: Connection,
    queue: EventQueue<WlState>,
    state: WlState,
}

struct CharSurf {
    layer: LayerSurface,
    output: Option<u32>,
    configured: bool,
    /// Margins (left, top) last sent, and the global position they stand for.
    margin: (i32, i32),
    requested: (i32, i32),
    /// Global position the compositor has applied, B-WL-7: set when the wl_display.sync
    /// sent after the margin commit is done. Pointer events before that `done` were
    /// generated with the old position, events after it with the new one.
    applied: (i32, i32),
    /// Distinguishes this surface's sync callbacks from those of a destroyed one.
    generation: u64,
    size: (u32, u32),
    keyboard: bool,
    buffer: Option<smithay_client_toolkit::shm::slot::Buffer>,
    attached: Option<Buf>,
    viewport: Option<wp_viewport::WpViewport>,
}

struct SubSurf {
    sub: wl_subsurface::WlSubsurface,
    surface: wl_surface::WlSurface,
    office: bool,
    buffer: Option<smithay_client_toolkit::shm::slot::Buffer>,
    attached: Option<(i32, i32, Buf)>,
}

struct MenuSurf {
    popup: Popup,
    configured: bool,
    /// Global top-left requested by the core (the compositor may flip or slide it).
    pos: (i32, i32),
    buffer: Option<smithay_client_toolkit::shm::slot::Buffer>,
    attached: Option<Buf>,
    geometry: bentwire_render::menu::MenuGeometry,
    entries: Vec<bentwire_core::api::MenuEntry>,
}

struct WlState {
    registry: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    sub: SubcompositorState,
    layer_shell: LayerShell,
    xdg: XdgShell,
    shm: Shm,
    pool: SlotPool,
    viewporter: Option<wp_viewporter::WpViewporter>,
    qh: QueueHandle<WlState>,
    display: wayland_client::protocol::wl_display::WlDisplay,
    generation: u64,
    ch: Option<CharSurf>,
    /// Agent balloon (0) and Office balloon (1).
    balloons: [Option<SubSurf>; 2],
    menu: Option<MenuSurf>,
    /// The latest scene from the app; re-applied after configures.
    desired: Rendered,
    events: Vec<Event>,
    outputs_dirty: bool,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    modifiers: u32,
    press_serial: u32,
    /// Which surface the pointer is on (for press/release routing).
    pointer_surface: Option<wl_surface::WlSurface>,
    /// Surface of the press holding the implicit grab, and its button.
    grab: Option<(wl_surface::WlSurface, u32)>,
    relative: RelativePointerState,
    rel_pointer: Option<ZwpRelativePointerV1>,
    /// Global pointer position during a grab (B-WL-7, A-DESK-5).
    grab_pos: Option<GrabPos>,
}

/// A wl_display.sync after a margin commit (B-WL-7).
struct SyncData {
    generation: u64,
    pos: (i32, i32),
}

impl Dispatch2<wayland_client::protocol::wl_callback::WlCallback, WlState> for SyncData {
    fn event(
        &self,
        state: &mut WlState,
        _: &wayland_client::protocol::wl_callback::WlCallback,
        _: wayland_client::protocol::wl_callback::Event,
        _: &Connection,
        _: &QueueHandle<WlState>,
    ) {
        if let Some(c) = &mut state.ch
            && c.generation == self.generation
        {
            c.applied = self.pos;
        }
    }
}

/// Viewport objects have no events.
struct ViewportData;

impl<D> Dispatch2<wp_viewporter::WpViewporter, D> for ViewportData {
    fn event(
        &self,
        _: &mut D,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}

impl<D> Dispatch2<wp_viewport::WpViewport, D> for ViewportData {
    fn event(
        &self,
        _: &mut D,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}

impl Wayland {
    pub fn connect() -> Result<Wayland, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("Wayland: {e}"))?;
        let (globals, queue) =
            registry_queue_init::<WlState>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
        let qh = queue.handle();
        // B-WL-2: zwlr_layer_shell_v1 version 4 or later is required.
        let ls_version = globals.contents().with_list(|l| {
            l.iter()
                .find(|g| g.interface == "zwlr_layer_shell_v1")
                .map(|g| g.version)
        });
        match ls_version {
            None => return Err("the compositor has no wlr-layer-shell (zwlr_layer_shell_v1); bentwired needs KDE Plasma, sway, Hyprland or another layer-shell compositor".into()),
            Some(v) if v < 4 => return Err(format!("the compositor's zwlr_layer_shell_v1 is version {v}; bentwired needs version 4 or later")),
            _ => {}
        }
        let compositor =
            CompositorState::bind(&globals, &qh).map_err(|e| format!("wl_compositor: {e}"))?;
        let sub = SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh)
            .map_err(|e| format!("wl_subcompositor: {e}"))?;
        let layer_shell =
            LayerShell::bind(&globals, &qh).map_err(|e| format!("layer shell: {e}"))?;
        let xdg = XdgShell::bind(&globals, &qh).map_err(|e| format!("xdg_wm_base: {e}"))?;
        let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm: {e}"))?;
        let viewporter = globals.bind(&qh, 1..=1, ViewportData).ok();
        let pool = SlotPool::new(256 * 256 * 4, &shm).map_err(|e| format!("shm pool: {e}"))?;
        let mut state = WlState {
            registry: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            compositor,
            sub,
            layer_shell,
            xdg,
            shm,
            pool,
            viewporter,
            qh: qh.clone(),
            display: conn.display(),
            generation: 0,
            ch: None,
            balloons: [None, None],
            menu: None,
            desired: Rendered::default(),
            events: Vec::new(),
            outputs_dirty: true,
            seat: None,
            pointer: None,
            keyboard: None,
            modifiers: 0,
            press_serial: 0,
            pointer_surface: None,
            grab: None,
            relative: RelativePointerState::bind(&globals, &qh),
            rel_pointer: None,
            grab_pos: None,
        };
        let mut queue = queue;
        // Two roundtrips: globals, then output and seat details.
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        Ok(Wayland { conn, queue, state })
    }

    pub fn fd(&self) -> RawFd {
        self.conn.as_fd().as_raw_fd()
    }

    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.state.outputs()
    }
}

impl Backend for Wayland {
    fn present(&mut self, r: &Rendered, _scene: &Scene) {
        self.state.desired = r.clone();
        self.state.apply();
        let _ = self.conn.flush();
    }

    fn poll(&mut self) -> Vec<Event> {
        let _ = self.queue.dispatch_pending(&mut self.state);
        if let Some(guard) = self.queue.prepare_read() {
            let _ = guard.read();
        }
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            log::error!("Wayland connection lost: {e}");
            std::process::exit(1);
        }
        if self.state.outputs_dirty {
            self.state.outputs_dirty = false;
            self.state.events.push(Event::Outputs(self.state.outputs()));
        }
        let _ = self.conn.flush();
        std::mem::take(&mut self.state.events)
    }

    fn flush(&mut self) {
        let _ = self.conn.flush();
    }

    fn snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        let s = &self.state;
        json!({
            "character": s.ch.as_ref().map(|c| json!({
                "output": c.output,
                "configured": c.configured,
                "margins_requested": {"left": c.margin.0, "top": c.margin.1},
                "position_applied": [c.applied.0, c.applied.1],
                "size": [c.size.0, c.size.1],
                "keyboard_interactivity": if c.keyboard { "on_demand" } else { "none" },
                "buffer_attached": c.attached.is_some(),
                "buffer_scale": c.attached.as_ref().map(|b| b.scale),
                "viewport": c.viewport.is_some(),
            })),
            "balloons": s.balloons.iter().flatten().map(|b| json!({
                "office": b.office,
                "position": b.attached.as_ref().map(|a| [a.0, a.1]),
                "buffer_attached": b.attached.is_some(),
            })).collect::<Vec<_>>(),
            "menu": s.menu.as_ref().map(|m| json!({
                "configured": m.configured,
                "buffer_attached": m.attached.is_some(),
            })),
            "outputs": s.outputs().iter().map(|o| json!({"id": o.id, "x": o.rect.x, "y": o.rect.y, "w": o.rect.w, "h": o.rect.h, "scale": o.scale})).collect::<Vec<_>>(),
        })
    }
}

/// The mode size in the output's orientation: a 90 or 270 degree transform swaps axes.
fn oriented((w, h): (i32, i32), t: wl_output::Transform) -> (i32, i32) {
    use wl_output::Transform as T;
    match t {
        T::_90 | T::_270 | T::Flipped90 | T::Flipped270 => (h, w),
        _ => (w, h),
    }
}

/// B-WL-6: the output scale, fractional from the (oriented) mode width over the
/// xdg-output logical width, else the integer wl_output scale.
fn output_scale(mode: Option<(i32, i32)>, logical: Option<(i32, i32)>, integer: i32) -> f64 {
    match (mode, logical) {
        (Some((mw, _)), Some((lw, _))) if lw > 0 => {
            let s = mw as f64 / lw as f64;
            if (s - s.round()).abs() < 0.01 {
                s.round()
            } else {
                s
            }
        }
        _ => integer as f64,
    }
}

fn button_code(b: u32) -> u32 {
    match b {
        BTN_LEFT => 1,
        BTN_RIGHT => 2,
        BTN_MIDDLE => 4,
        _ => 0,
    }
}

impl WlState {
    fn outputs(&self) -> Vec<OutputInfo> {
        let mut v: Vec<OutputInfo> = self
            .output_state
            .outputs()
            .filter_map(|o| self.output_state.info(&o))
            .filter_map(|i| {
                let (x, y) = i.logical_position.unwrap_or(i.location);
                let mode = i
                    .modes
                    .iter()
                    .find(|m| m.current)
                    .map(|m| oriented(m.dimensions, i.transform));
                let (w, h) = i.logical_size.or(mode).unwrap_or((0, 0));
                if w <= 0 || h <= 0 {
                    return None;
                }
                let scale = output_scale(mode, i.logical_size, i.scale_factor);
                Some(OutputInfo {
                    id: i.id,
                    rect: Rect::new(x, y, w, h),
                    scale,
                })
            })
            .collect();
        v.sort_by_key(|o| (o.rect.x, o.rect.y, o.id));
        v
    }

    fn wl_output(&self, id: Option<u32>) -> Option<wl_output::WlOutput> {
        let id = id?;
        self.output_state
            .outputs()
            .find(|o| self.output_state.info(o).is_some_and(|i| i.id == id))
    }

    fn destroy_menu(&mut self) {
        self.menu = None;
    }

    fn destroy_balloon(&mut self, i: usize) {
        if let Some(b) = self.balloons[i].take() {
            b.sub.destroy();
            b.surface.destroy();
        }
    }

    fn destroy_character(&mut self) {
        self.destroy_menu();
        self.destroy_balloon(1);
        self.destroy_balloon(0);
        if let Some(c) = self.ch.take() {
            if let Some(v) = c.viewport {
                v.destroy();
            }
            drop(c.layer);
        }
    }

    /// Make the compositor state match `desired`.
    fn apply(&mut self) {
        let Some(want) = self.desired.character.clone() else {
            self.destroy_character();
            return;
        };
        // B-WL-4: a new output means a new surface (never during a drag; the core keeps
        // `output` fixed while dragging).
        if self.ch.as_ref().is_some_and(|c| c.output != want.output) {
            self.destroy_character();
        }
        if self.ch.is_none() {
            let surface = self.compositor.create_surface(&self.qh);
            let out = self.wl_output(want.output);
            let layer = self.layer_shell.create_layer_surface(
                &self.qh,
                surface,
                Layer::Top,
                Some("bentwire"),
                out.as_ref(),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            let viewport = None;
            self.ch = Some(CharSurf {
                layer,
                output: want.output,
                configured: false,
                margin: (i32::MIN, i32::MIN),
                requested: (want.x, want.y),
                applied: (want.x, want.y),
                generation: {
                    self.generation += 1;
                    self.generation
                },
                size: (0, 0),
                keyboard: false,
                buffer: None,
                attached: None,
                viewport,
            });
        }
        let origin = want.origin;
        let kb = self.desired.keyboard;
        let qh = self.qh.clone();
        let viewporter = self.viewporter.clone();
        let c = self.ch.as_mut().expect("created above");
        let mut dirty = false;
        let mut sync = None;
        // B-WL-2: margins = top-left minus the output origin (not clamped, B-WL-4). Each
        // change is followed by a sync, so pointer positions are mapped with the position
        // the compositor has applied (B-WL-7).
        let margin = (want.x - origin.0, want.y - origin.1);
        if c.margin != margin {
            c.layer.set_margin(margin.1, 0, 0, margin.0);
            c.margin = margin;
            c.requested = (want.x, want.y);
            if c.configured {
                sync = Some(SyncData {
                    generation: c.generation,
                    pos: (want.x, want.y),
                });
            }
            dirty = true;
        }
        if c.size != (want.buf.w, want.buf.h) {
            c.layer.set_size(want.buf.w, want.buf.h);
            c.size = (want.buf.w, want.buf.h);
            dirty = true;
        }
        if c.keyboard != kb {
            // B-WL-8: on_demand only while an ask balloon is open.
            c.layer.set_keyboard_interactivity(if kb {
                KeyboardInteractivity::OnDemand
            } else {
                KeyboardInteractivity::None
            });
            c.keyboard = kb;
            dirty = true;
        }
        if !c.configured {
            // Initial commit without a buffer; the configure handler re-runs apply().
            c.layer.commit();
            return;
        }
        let wl = c.layer.wl_surface().clone();
        let fractional = want.viewport && viewporter.is_some();
        let c = self.ch.as_mut().expect("present");
        if c.attached.as_ref() != Some(&want.buf) || c.viewport.is_some() != fractional {
            match attach(&mut self.pool, &wl, &want.buf) {
                Some(b) => {
                    set_input(&self.compositor, &wl, &want.buf.input);
                    if let (true, Some(vp)) = (fractional, &viewporter) {
                        let v = c
                            .viewport
                            .get_or_insert_with(|| vp.get_viewport(&wl, &qh, ViewportData));
                        wl.set_buffer_scale(1);
                        v.set_destination(want.buf.w as i32, want.buf.h as i32);
                    } else {
                        if let Some(v) = c.viewport.take() {
                            v.destroy();
                        }
                        wl.set_buffer_scale(want.buf.scale as i32);
                    }
                    c.buffer = Some(b);
                    c.attached = Some(want.buf.clone());
                    dirty = true;
                }
                None => log::warn!("could not allocate a character buffer"),
            }
        }
        self.apply_balloon(0, &wl, (want.x, want.y), &mut dirty);
        self.apply_balloon(1, &wl, (want.x, want.y), &mut dirty);
        self.apply_menu(&wl, (want.x, want.y));
        if dirty {
            let c = self.ch.as_mut().expect("present");
            c.layer.commit();
            if let Some(d) = sync {
                self.display.sync(&qh, d);
            }
        }
    }

    fn apply_balloon(
        &mut self,
        i: usize,
        parent: &wl_surface::WlSurface,
        at: (i32, i32),
        dirty: &mut bool,
    ) {
        let Some(want) = self.desired.balloons[i].clone() else {
            if self.balloons[i].is_some() {
                self.destroy_balloon(i);
                *dirty = true;
            }
            return;
        };
        if self.balloons[i].is_none() {
            let (sub, surface) = self.sub.create_subsurface(parent.clone(), &self.qh);
            match &self.balloons[0] {
                Some(agent) if i == 1 => sub.place_above(&agent.surface),
                _ => sub.place_above(parent),
            }
            self.balloons[i] = Some(SubSurf {
                sub,
                surface,
                office: want.office,
                buffer: None,
                attached: None,
            });
        }
        let b = self.balloons[i].as_mut().expect("created above");
        let rel = (want.x - at.0, want.y - at.1);
        if b.attached.as_ref().map(|a| (a.0, a.1, &a.2)) == Some((rel.0, rel.1, &want.buf)) {
            return;
        }
        // B-WL-8: positioned relative to the character; applied on the parent's commit.
        b.sub.set_position(rel.0, rel.1);
        if b.attached.as_ref().map(|a| &a.2) != Some(&want.buf) {
            match attach(&mut self.pool, &b.surface, &want.buf) {
                Some(buf) => {
                    set_input(&self.compositor, &b.surface, &want.buf.input);
                    b.surface.set_buffer_scale(want.buf.scale as i32);
                    b.buffer = Some(buf);
                }
                None => log::warn!("could not allocate a balloon buffer"),
            }
        }
        b.surface.commit();
        b.attached = Some((rel.0, rel.1, want.buf.clone()));
        *dirty = true;
    }

    fn apply_menu(&mut self, _parent: &wl_surface::WlSurface, at: (i32, i32)) {
        let Some(want) = self.desired.menu.clone() else {
            self.destroy_menu();
            return;
        };
        if self.menu.is_none() {
            let Ok(pos) = XdgPositioner::new(&self.xdg) else {
                return;
            };
            pos.set_size(want.buf.w as i32, want.buf.h as i32);
            // B-WL-9: at the pointer, flipped and slid to stay on the output.
            pos.set_anchor_rect(want.x - at.0, want.y - at.1, 1, 1);
            pos.set_anchor(xdg_positioner::Anchor::TopLeft);
            pos.set_gravity(xdg_positioner::Gravity::BottomRight);
            pos.set_constraint_adjustment(
                xdg_positioner::ConstraintAdjustment::FlipX
                    | xdg_positioner::ConstraintAdjustment::FlipY
                    | xdg_positioner::ConstraintAdjustment::SlideX
                    | xdg_positioner::ConstraintAdjustment::SlideY,
            );
            let surface = self.compositor.create_surface(&self.qh);
            let Ok(popup) = Popup::from_surface(None, &pos, &self.qh, surface, &self.xdg) else {
                log::warn!("menu: popup creation failed");
                return;
            };
            if let Some(c) = &self.ch {
                c.layer.get_popup(popup.xdg_popup());
            }
            // Grabbed with the serial of the opening press. A menu opened by a synthetic
            // Test1 click has no serial and is shown without a grab.
            if let (Some(seat), true) = (&self.seat, self.press_serial != 0) {
                popup.xdg_popup().grab(seat, self.press_serial);
            }
            popup.wl_surface().commit();
            self.menu = Some(MenuSurf {
                popup,
                configured: false,
                pos: (want.x, want.y),
                buffer: None,
                attached: None,
                geometry: want.geometry.clone(),
                entries: want.entries.clone(),
            });
            return;
        }
        let m = self.menu.as_mut().expect("checked");
        m.geometry = want.geometry.clone();
        m.entries = want.entries.clone();
        m.pos = (want.x, want.y);
        if !m.configured || m.attached.as_ref() == Some(&want.buf) {
            return;
        }
        let wl = m.popup.wl_surface().clone();
        if let Some(b) = attach(&mut self.pool, &wl, &want.buf) {
            wl.set_buffer_scale(want.buf.scale as i32);
            m.buffer = Some(b);
            m.attached = Some(want.buf.clone());
            wl.commit();
        }
    }

    /// Global logical origin of a surface of ours, and which core surface it is.
    fn surface_info(&self, s: &wl_surface::WlSurface) -> Option<(SurfKind, (i32, i32))> {
        if let Some(c) = &self.ch
            && c.layer.wl_surface() == s
        {
            return Some((SurfKind::Core(CoreSurface::Character), c.applied));
        }
        if let (Some(b), Some(c)) = (
            self.balloons.iter().flatten().find(|b| &b.surface == s),
            &self.ch,
        ) {
            let rel = b.attached.as_ref().map(|a| (a.0, a.1)).unwrap_or((0, 0));
            let kind = if b.office {
                CoreSurface::OfficeBalloon
            } else {
                CoreSurface::AgentBalloon
            };
            return Some((
                SurfKind::Core(kind),
                (c.applied.0 + rel.0, c.applied.1 + rel.1),
            ));
        }
        if let Some(m) = &self.menu
            && m.popup.wl_surface() == s
        {
            return Some((SurfKind::Menu, (0, 0)));
        }
        None
    }

    fn menu_item(&self, local: (f64, f64)) -> Option<usize> {
        let m = self.menu.as_ref()?;
        m.geometry.hit(&m.entries, local.0 as i32, local.1 as i32)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SurfKind {
    Core(CoreSurface),
    Menu,
}

/// Copy a buffer into the pool and attach it (premultiplied ARGB8888, B-WL-6).
fn attach(
    pool: &mut SlotPool,
    s: &wl_surface::WlSurface,
    b: &Buf,
) -> Option<smithay_client_toolkit::shm::slot::Buffer> {
    let (w, h) = ((b.w * b.scale) as i32, (b.h * b.scale) as i32);
    if w == 0 || h == 0 {
        return None;
    }
    let (buffer, canvas) = pool
        .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        .ok()?;
    let n = canvas.len().min(b.argb.len());
    canvas[..n].copy_from_slice(&b.argb[..n]);
    buffer.attach_to(s).ok()?;
    s.damage_buffer(0, 0, w, h);
    Some(buffer)
}

/// B-WL-6: the input region is the shape.
fn set_input(
    c: &CompositorState,
    s: &wl_surface::WlSurface,
    rects: &[bentwire_render::canvas::Rect],
) {
    if let Ok(r) = Region::new(c) {
        for q in rects {
            r.add(q.x, q.y, q.w, q.h);
        }
        s.set_input_region(Some(r.wl_region()));
    }
}

fn key_of(e: &KeyEvent) -> Option<Key> {
    Some(match e.keysym {
        Keysym::Escape => Key::Escape,
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::space => Key::Space,
        Keysym::Up => Key::Up,
        Keysym::Down => Key::Down,
        Keysym::Left => Key::Left,
        Keysym::Right => Key::Right,
        _ => e
            .keysym
            .key_char()
            .or_else(|| e.utf8.as_deref().and_then(|s| s.chars().next()))
            .filter(|c| !c.is_control())
            .map(Key::Char)?,
    })
}

impl CompositorHandler for WlState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for WlState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_dirty = true;
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_dirty = true;
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_dirty = true;
    }
}

impl LayerShellHandler for WlState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        // The compositor dropped the surface (e.g. its output went away); recreate it.
        if self.ch.as_ref().is_some_and(|c| &c.layer == layer) {
            self.destroy_character();
            self.apply();
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(c) = &mut self.ch
            && &c.layer == layer
            && !c.configured
        {
            c.configured = true;
            c.applied = c.requested;
            self.apply();
        }
    }
}

impl PopupHandler for WlState {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        _: PopupConfigure,
    ) {
        if let Some(m) = &mut self.menu
            && m.popup.wl_surface() == popup.wl_surface()
        {
            m.configured = true;
            m.attached = None;
            if let Some(c) = &self.ch {
                let wl = c.layer.wl_surface().clone();
                let at = c.applied;
                self.apply_menu(&wl, at);
            }
        }
    }
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        // B-WL-9: popup_done closes the menu without a choice.
        if self
            .menu
            .as_ref()
            .is_some_and(|m| m.popup.wl_surface() == popup.wl_surface())
        {
            self.menu = None;
            self.events.push(Event::Menu(MenuInput::Dismiss));
        }
    }
}

impl SeatHandler for WlState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seat.get_or_insert(seat);
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        self.seat.get_or_insert(seat.clone());
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            if let Some(p) = &self.pointer {
                self.rel_pointer = self.relative.get_relative_pointer(p, qh).ok();
                if self.rel_pointer.is_none() {
                    log::info!("no zwp_relative_pointer_manager_v1: drags use absolute motion");
                }
            }
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
        if cap == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            if let Some(r) = self.rel_pointer.take() {
                r.destroy();
            }
            self.grab_pos = None;
            p.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for WlState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        e: KeyEvent,
    ) {
        if let Some(key) = key_of(&e) {
            self.events.push(Event::Key {
                key,
                modifiers: self.modifiers,
            });
        }
    }
    fn repeat_key(
        &mut self,
        c: &Connection,
        q: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        s: u32,
        e: KeyEvent,
    ) {
        self.press_key(c, q, k, s, e);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        m: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        // B-UI-1: 1 Shift, 2 Ctrl, 4 Alt.
        self.modifiers = (m.shift as u32) | (m.ctrl as u32) << 1 | (m.alt as u32) << 2;
    }
}

impl PointerHandler for WlState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for ev in events {
            let Some((kind, origin)) = self.surface_info(&ev.surface) else {
                continue;
            };
            let local = ev.position;
            let mut pos = (origin.0 as f64 + local.0, origin.1 as f64 + local.1);
            // B-WL-7, A-DESK-5: during a grab the position follows relative-pointer deltas
            // when the compositor sends them, else absolute motion. A release reuses the
            // last grab position (it carries no new coordinates).
            match &ev.kind {
                PointerEventKind::Press { .. } if matches!(kind, SurfKind::Core(_)) => {
                    self.grab_pos = Some(GrabPos::new(pos));
                }
                PointerEventKind::Motion { .. } => {
                    if let (Some(g), Some(_)) = (&mut self.grab_pos, &self.grab) {
                        pos = g.motion(local, origin);
                        log::debug!(
                            "grab motion delta={:?} local={local:?} applied={origin:?} -> {pos:?}",
                            g.delta
                        );
                    }
                }
                PointerEventKind::Release { .. } => {
                    if let (Some(g), Some(_)) = (&self.grab_pos, &self.grab) {
                        pos = g.pos;
                    }
                }
                _ => {}
            }
            let (x, y) = (pos.0.floor() as i32, pos.1.floor() as i32);
            let core = |k: PointerKind, button: u32, surface: CoreSurface, modifiers: u32| {
                Event::Pointer(CoreEvent {
                    kind: k,
                    surface,
                    button,
                    modifiers,
                    x,
                    y,
                })
            };
            match (&ev.kind, kind) {
                (PointerEventKind::Enter { .. }, _) => {
                    self.pointer_surface = Some(ev.surface.clone())
                }
                (PointerEventKind::Leave { .. }, SurfKind::Core(s)) => {
                    self.pointer_surface = None;
                    if self.grab.is_none() {
                        self.events
                            .push(core(PointerKind::Leave, 0, s, self.modifiers));
                    }
                }
                (PointerEventKind::Motion { .. }, SurfKind::Menu) => {
                    let i = self.menu_item(local);
                    self.events.push(Event::Menu(MenuInput::Hover(i)));
                }
                (PointerEventKind::Motion { .. }, SurfKind::Core(s)) => {
                    let b = self.grab.as_ref().map(|g| g.1).unwrap_or(0);
                    self.events
                        .push(core(PointerKind::Motion, b, s, self.modifiers));
                }
                (PointerEventKind::Press { button, serial, .. }, SurfKind::Core(s)) => {
                    self.press_serial = *serial;
                    let b = button_code(*button);
                    if b == 0 {
                        continue;
                    }
                    self.grab = Some((ev.surface.clone(), b));
                    self.events
                        .push(core(PointerKind::Press, b, s, self.modifiers));
                }
                (PointerEventKind::Release { button, .. }, SurfKind::Core(s)) => {
                    let b = button_code(*button);
                    if b == 0 {
                        continue;
                    }
                    self.grab = None;
                    self.grab_pos = None;
                    self.events
                        .push(core(PointerKind::Release, b, s, self.modifiers));
                }
                (PointerEventKind::Release { .. }, SurfKind::Menu) => {
                    if let Some(i) = self.menu_item(local) {
                        self.events.push(Event::Menu(MenuInput::Activate(i)));
                    }
                }
                _ => {}
            }
        }
    }
}

/// B-WL-7: pointer position during an implicit grab. The press position and the sum of
/// relative-pointer deltas since the press are kept apart from the last reported position,
/// so absolute motion never feeds into the relative sum (the protocol does not order
/// wl_pointer.motion and relative_motion).
#[derive(Debug, Clone, Copy, PartialEq)]
struct GrabPos {
    press: (f64, f64),
    /// Sum of relative deltas; `None` until the first relative event of the grab.
    delta: Option<(f64, f64)>,
    /// Last reported global position (a release reuses it).
    pos: (f64, f64),
}

impl GrabPos {
    fn new(press: (f64, f64)) -> Self {
        Self {
            press,
            delta: None,
            pos: press,
        }
    }

    fn relative(&mut self, d: (f64, f64)) {
        let s = self.delta.get_or_insert((0.0, 0.0));
        s.0 += d.0;
        s.1 += d.1;
        self.pos = (self.press.0 + s.0, self.press.1 + s.1);
    }

    fn motion(&mut self, local: (f64, f64), applied: (i32, i32)) -> (f64, f64) {
        self.pos = grab_motion(self.press, self.delta, local, applied);
        self.pos
    }
}

/// B-WL-7: global position for a motion during a grab. With relative-pointer events
/// since the press, the press position plus their sum is used and the motion's own
/// coordinates are ignored: sway reports surface-local positions relative to the
/// press-time surface position. Without them (KWin nested, no manager) the absolute
/// motion maps with the applied origin, which compensates margins the compositor has not
/// applied yet.
fn grab_motion(
    press: (f64, f64),
    delta: Option<(f64, f64)>,
    local: (f64, f64),
    applied: (i32, i32),
) -> (f64, f64) {
    match delta {
        Some(d) => (press.0 + d.0, press.1 + d.1),
        None => (applied.0 as f64 + local.0, applied.1 as f64 + local.1),
    }
}

/// Required by XdgShell::bind; bentwired creates no toplevel windows.
impl smithay_client_toolkit::shell::xdg::window::WindowHandler for WlState {
    fn request_close(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &smithay_client_toolkit::shell::xdg::window::Window,
    ) {
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &smithay_client_toolkit::shell::xdg::window::Window,
        _: smithay_client_toolkit::shell::xdg::window::WindowConfigure,
        _: u32,
    ) {
    }
}

impl RelativePointerHandler for WlState {
    fn relative_pointer_motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpRelativePointerV1,
        _: &wl_pointer::WlPointer,
        e: RelativeMotionEvent,
    ) {
        if let Some(g) = &mut self.grab_pos {
            g.relative(e.delta);
        }
    }
}

impl ShmHandler for WlState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(WlState);

impl ProvidesRegistryState for WlState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(WlState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_wl_6_scale_of_rotated_and_fractional_outputs() {
        use wl_output::Transform;
        // 1920x1080 rotated by 90 degrees, scale 1: logical 1080x1920.
        let m = oriented((1920, 1080), Transform::_90);
        assert_eq!(m, (1080, 1920));
        assert_eq!(output_scale(Some(m), Some((1080, 1920)), 1), 1.0);
        let m = oriented((1920, 1080), Transform::Flipped270);
        assert_eq!(output_scale(Some(m), Some((1080, 1920)), 1), 1.0);
        // Not rotated: 1.5 stays fractional, 2 is exact.
        let m = oriented((1920, 1080), Transform::Normal);
        assert_eq!(output_scale(Some(m), Some((1280, 720)), 2), 1.5);
        assert_eq!(output_scale(Some(m), Some((960, 540)), 2), 2.0);
        assert_eq!(oriented((1920, 1080), Transform::_180), (1920, 1080));
        // No xdg-output: the integer scale.
        assert_eq!(output_scale(Some(m), None, 2), 2.0);
    }

    #[test]
    fn b_wl_7_grab_motion_relative_and_absolute() {
        // Relative mode, sway quirk: press at global (150, 120) on a surface at (100, 100),
        // drag by +50 px while the margin moves to 150: sway reports local 100 (relative to
        // the press-time origin) with applied 150. The deltas give 200, not 250.
        assert_eq!(
            grab_motion((150.0, 120.0), Some((50.0, 0.0)), (100.0, 20.0), (150, 100)),
            (200.0, 120.0)
        );
        // Absolute mode, margin pending: pointer at global 160 seen before the margin move
        // was applied (applied 100, local 60) and after it (applied 150, local 10).
        assert_eq!(
            grab_motion((150.0, 120.0), None, (60.0, 20.0), (100, 100)),
            (160.0, 120.0)
        );
        assert_eq!(
            grab_motion((150.0, 120.0), None, (10.0, 20.0), (150, 100)),
            (160.0, 120.0)
        );
    }

    #[test]
    fn b_wl_7_absolute_motion_before_first_relative_event() {
        // Press at (150, 120); the absolute motion of a +50 px move arrives before its
        // relative event. The delta must not count twice.
        let mut g = GrabPos::new((150.0, 120.0));
        assert_eq!(g.motion((100.0, 20.0), (100, 100)), (200.0, 120.0));
        g.relative((50.0, 0.0));
        assert_eq!(g.pos, (200.0, 120.0));
        // The next motion follows press plus deltas, whatever it reports (sway quirk).
        g.relative((10.0, 0.0));
        assert_eq!(g.motion((110.0, 20.0), (150, 100)), (210.0, 120.0));
        assert_eq!(g.pos, (210.0, 120.0));
    }
}
