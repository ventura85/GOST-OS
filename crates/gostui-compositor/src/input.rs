//! Routing input: from a device event to either the shell or a client.
//!
//! Everything here is translation, in the same sense as `crate::wayland`. What a
//! point on screen *means* is [`gostui_core::input::hit_test`], what a key
//! combination *does* is [`gostui_core::input::Keymap`], and what focus moving
//! does to the tiles is [`gostui_core::WindowModel`]. This module carries the
//! answers to the protocol and nowhere else — if a decision about where input
//! goes gets written here, it is in the wrong file (D-016).
//!
//! # Three paths, on purpose (D-020, D-022)
//!
//! Keyboard, pointer and touch are handled separately all the way down, and
//! `wl_touch` is never a renamed `wl_pointer`. The pointer *mode* of D-022 — the
//! virtual trackpad that makes desktop applications usable with a finger — is a
//! mode the user switches on, needs a control that does not exist yet, and is
//! M3. Keeping the paths apart now is what lets it be written then without
//! unpicking this.
//!
//! # What must not happen here
//!
//! **Pointer motion must not draw a frame.** The shell's picture does not depend
//! on where the cursor is, so moving the mouse across a window redraws nothing;
//! the client gets its motion events and that is all. A `request_redraw` on the
//! motion path would be several hundred frames a second and would quietly repeal
//! the rule the whole `stats` module exists to police (D-027).
//!
//! # The cursor
//!
//! Not drawn. In the nested window the host session's cursor is the one on
//! screen, and drawing a second one under it would be two cursors. The
//! compositor draws its own from M4 (there is nothing else to draw it on a tty)
//! and needs it for D-022 — which is why `SeatHandler::cursor_image` is where
//! that lands, not here.

use crate::backend::winit::State;
use crate::stats::Cause;
use gostui_core::input::{hit_test, Action, Hit, Keysym, Mods};
use gostui_core::Point;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, TouchEvent, TouchSlot,
};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent};
use smithay::reexports::wayland_server::Resource as _;
use smithay::utils::{Logical, Point as SmithayPoint, SERIAL_COUNTER};
use smithay::wayland::pointer_constraints::{with_pointer_constraint, PointerConstraint};
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;

impl State {
    /// The entry point from the backend's event source.
    ///
    /// Generic over the input backend so that the DRM/libinput backend (M4) uses
    /// this same routing instead of growing a second copy of it.
    pub(crate) fn on_input<B: InputBackend>(&mut self, event: InputEvent<B>) {
        match event {
            InputEvent::Keyboard { event } => self.on_key::<B>(event),
            InputEvent::PointerMotionAbsolute { event } => {
                let point = self.absolute::<B, _>(&event);
                self.on_pointer_motion(point, event.time_msec(), event.time());
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<B>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<B>(event),
            InputEvent::TouchDown { event } => self.on_touch_down::<B>(event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(event),
            InputEvent::TouchCancel { .. } => self.on_touch_cancel(),
            InputEvent::TouchFrame { .. } => {
                if let Some(touch) = self.seat.get_touch() {
                    touch.frame(self);
                }
            }
            // Devices appearing and disappearing, relative motion from a real
            // mouse, gestures from a touchpad. The nested backend produces none
            // of them; the tty backend does, and each is its own step.
            _ => {}
        }
    }

    /// Absolute device coordinates to a point on our output.
    ///
    /// `x_transformed` wants the size to scale into, and the nested window runs
    /// at scale 1 (see `try_draw`), so the transform is the window size itself.
    fn absolute<B: InputBackend, E: AbsolutePositionEvent<B>>(
        &self,
        event: &E,
    ) -> SmithayPoint<f64, Logical> {
        let size = self.window_size();
        (
            event.x_transformed(size.w).clamp(0.0, size.w as f64),
            event.y_transformed(size.h).clamp(0.0, size.h as f64),
        )
            .into()
    }

    /// A key, on its way to a shortcut or to the focused window.
    fn on_key<B: InputBackend>(&mut self, event: B::KeyboardKeyEvent) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            // No keymap could be compiled at start-up. Logged there; dropping
            // the key here is better than a panic in a shell that is already
            // crippled.
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let time = event.time_msec();
        let pressed = event.state() == KeyState::Pressed;

        let action = keyboard.input(
            self,
            event.key_code(),
            event.state(),
            serial,
            time,
            |state, mods, handle| {
                if !pressed {
                    // Only presses are shortcuts. Intercepting the release too
                    // would be tidier in theory and wrong in practice: the client
                    // never saw the press, so it has nothing to release.
                    return FilterResult::Forward;
                }
                let mods = Mods::from_flags(mods.shift, mods.ctrl, mods.alt, mods.logo);
                // `RUST_LOG=gostui=debug` prints every press with the symbol and
                // the modifiers the shell believes are held. Worth keeping: when
                // a shortcut "does nothing", the answer is always one of three —
                // the key never arrived, the modifier was not seen, or the symbol
                // was not the one the binding names — and this line says which.
                tracing::debug!(
                    keysym = format!("{:#x}", handle.modified_sym().raw()),
                    ?mods,
                    "key pressed"
                );
                // Both the symbol the layout produced and the raw ones: with
                // Shift held, xkb turns Tab into ISO_Left_Tab, and a binding
                // written as Super+Shift+Tab would never fire if we only asked
                // for the modified symbol.
                let mut candidates = vec![Keysym(handle.modified_sym().raw())];
                candidates.extend(handle.raw_syms().iter().map(|s| Keysym(s.raw())));
                match candidates
                    .into_iter()
                    .find_map(|key| state.keymap.action(key, mods))
                {
                    Some(action) => FilterResult::Intercept(action),
                    None => FilterResult::Forward,
                }
            },
        );

        if let Some(action) = action {
            self.run(action);
        }
    }

    /// Tell a client that the frame around its window is ours to draw.
    ///
    /// Sent on every request rather than once, because a client may ask again at
    /// any time and the answer never changes (D-025).
    pub(crate) fn decorate(&mut self, toplevel: &smithay::wayland::shell::xdg::ToplevelSurface) {
        use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_pending_configure();
    }

    /// Where a popup's parent window is on screen, if it is visible.
    ///
    /// `None` when the parent holds no tile: a menu belonging to a window
    /// waiting on the bottom bar has nowhere to be, and placing it against a
    /// rectangle that is not on screen would put it in a corner of its own.
    pub(crate) fn parent_rect(
        &self,
        popup: &smithay::wayland::shell::xdg::PopupSurface,
    ) -> Option<gostui_core::Rect> {
        let parent = popup.get_parent_surface()?;
        let window = self.wayland.window_of(&parent)?;
        self.placed_windows()
            .iter()
            .find(|p| p.window == window)
            .map(|p| p.rect)
    }

    /// Carry out a shell action.
    fn run(&mut self, action: Action) {
        tracing::info!(?action, "shell shortcut");
        match action {
            Action::FocusNextWindow => self.cycle_focus(true),
            Action::FocusPreviousWindow => self.cycle_focus(false),
            Action::CloseWindow => {
                let Some(window) = self.windows.focused() else {
                    return;
                };
                // `close` is a request, not an order: an editor with unsaved work
                // is entitled to put up a dialog instead. Nothing is removed from
                // the model here — the window goes away when the client destroys
                // the toplevel, and `toplevel_destroyed` handles that already.
                if let Some(toplevel) = self.wayland.toplevel_of(window) {
                    toplevel.send_close();
                }
            }
            Action::ToggleFullscreen => {
                let Some(window) = self.windows.focused() else {
                    return;
                };
                let Some(w) = self.windows.get_mut(window) else {
                    return;
                };
                w.fullscreen = !w.fullscreen;
                // The shell's own state, flipped here rather than asked of the
                // client: a window covering both bars must be escapable even
                // when the application has stopped answering. `sync_layout`
                // sends the client the new size and the `Fullscreen` flag, and
                // it is free to redraw accordingly — but it does not get a vote.
                self.focus_changed();
            }
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        if self.windows.cycle_focus(self.output, forward).is_some() {
            self.focus_changed();
        }
    }

    /// Everything that has to happen when focus moves.
    ///
    /// Three things, and forgetting any one of them is a different bug: the
    /// layout may have changed (a waiting window took a tile), the clients have
    /// to be told who is activated, and the keyboard has to start going
    /// somewhere else.
    pub(crate) fn focus_changed(&mut self) {
        self.sync_layout();
        self.refresh_keyboard_focus();
        self.draw(Cause::Input);
    }

    /// Point the keyboard at the focused window's surface.
    ///
    /// Called after every focus change and after every layout pass, because a
    /// window can lose its tile without anybody touching the keyboard — and a
    /// keyboard still aimed at a window that is not on screen types into
    /// something the user cannot see.
    pub(crate) fn refresh_keyboard_focus(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        // Only a window that is actually on screen may take the keyboard: the
        // one waiting on the bottom bar is not, and typing into something
        // invisible is worse than typing into nothing.
        //
        // "On screen" is asked of the layout rather than of the tile list,
        // because a dialog holds no tile and must still be typed into — a "Save
        // as" you cannot type a filename into is the whole trap of D-025 in one
        // window.
        let placed = self.placed_windows();
        let visible = self
            .windows
            .focused()
            .filter(|w| placed.iter().any(|p| p.window == *w));
        let surface = visible.and_then(|w| self.wayland.surface_of(w).cloned());
        if surface == keyboard.current_focus() {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        keyboard.set_focus(self, surface.clone(), serial);

        // The clipboard follows the keyboard, and saying so is a separate call.
        //
        // smithay moves the data by itself, but it will not hand a selection to
        // a client it has not been told is entitled to one — the rule that stops
        // a background application reading what the user copied. Without these
        // two lines a paste is silence: measured 2026-08-02, `wl-copy` set the
        // selection and `wl-paste` received `wl_keyboard.enter` and then waited
        // for a `wl_data_device.selection` that was never sent.
        //
        // Both selections, because wayland keeps the clipboard and the
        // middle-click selection apart and users rely on the difference.
        let dh = self.wayland.display.clone();
        let seat = self.seat.clone();
        let client = surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(&dh, &seat, client.clone());
        set_primary_focus(&dh, &seat, client);
    }

    /// The pointer moved. Nothing the shell draws depends on this.
    fn on_pointer_motion(&mut self, location: SmithayPoint<f64, Logical>, time: u32, utime: u64) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let previous = self.pointer_location;

        // A locked pointer keeps its position: the client asked for the cursor to
        // stand still and to be told the movement instead (D-022 — this is what
        // a first-person game and a virtual trackpad both need). Confinement is
        // *not* honoured here and therefore never activated, because clamping to
        // a region we do not implement would be worse than not offering it.
        if !self.pointer_locked() {
            self.pointer_location = location;
        }

        let focus = self.pointer_focus();
        // `RUST_LOG=gostui=trace`: says whether the pointer resolved to a client
        // surface at all. "clicking does nothing" has exactly two causes — the
        // event never arrived, or it arrived and went nowhere — and this tells
        // them apart without guessing.
        tracing::trace!(
            x = self.pointer_location.x,
            y = self.pointer_location.y,
            focus = focus.is_some(),
            "pointer motion"
        );
        pointer.motion(
            self,
            focus.clone(),
            &MotionEvent {
                location: self.pointer_location,
                serial,
                time,
            },
        );
        let delta = (location.x - previous.x, location.y - previous.y);
        if delta != (0.0, 0.0) {
            pointer.relative_motion(
                self,
                focus,
                &RelativeMotionEvent {
                    delta: delta.into(),
                    // No acceleration curve of our own on the nested backend: the
                    // host session already applied one, and applying a second
                    // would make the pointer feel different inside our window
                    // than outside it.
                    delta_unaccel: delta.into(),
                    utime,
                },
            );
        }
        pointer.frame(self);
        // Deliberately no `request_redraw`. See the module docs.
    }

    /// True when the surface under the pointer holds an active lock.
    fn pointer_locked(&self) -> bool {
        let Some(pointer) = self.seat.get_pointer() else {
            return false;
        };
        let Some(surface) = pointer.current_focus() else {
            return false;
        };
        with_pointer_constraint(&surface, &pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active() && matches!(&*c, PointerConstraint::Locked(_)))
        })
    }

    fn on_pointer_button<B: InputBackend>(&mut self, event: B::PointerButtonEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let state = event.state();

        tracing::trace!(
            button = event.button_code(),
            ?state,
            hit = ?self.hit(self.pointer_point()),
            "pointer button"
        );
        if state == ButtonState::Pressed {
            // The press is what moves focus — click to focus, not focus follows
            // the pointer. A window must not change under a cursor crossing it on
            // its way somewhere else, and on a touchscreen there is no such thing
            // as hovering to begin with.
            self.press(self.pointer_point());
        }

        // Sent regardless of what the press did to focus: the pointer's own focus
        // is whatever the last motion put it on, and when that is a bar it is
        // nothing, so nothing is forwarded. That is the shell keeping its zones
        // to itself, not a special case.
        pointer.button(
            self,
            &ButtonEvent {
                serial,
                time: event.time_msec(),
                button: event.button_code(),
                state,
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_axis<B: InputBackend>(&mut self, event: B::PointerAxisEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let source = event.source();
        let mut frame = AxisFrame::new(event.time_msec()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                if let Some(discrete) = event.amount_v120(axis) {
                    frame = frame.v120(axis, discrete as i32);
                }
                frame = frame.value(axis, amount);
            } else if let Some(discrete) = event.amount_v120(axis) {
                // A mouse wheel reports steps and no continuous value. 15 units
                // per notch is what the protocol's own documentation uses.
                frame = frame.v120(axis, discrete as i32);
                frame = frame.value(axis, discrete / 120.0 * 15.0);
            }
            if event.amount(axis) == Some(0.0) && source == AxisSource::Finger {
                frame = frame.stop(axis);
            }
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    /// What a press at this point does to the shell, wherever the press came
    /// from.
    ///
    /// Shared by the pointer and the touch path because the *decision* is the
    /// same one — a chip is a chip whether it was clicked or tapped. The protocol
    /// paths stay separate, which is the part that matters (D-020).
    fn press(&mut self, point: Point) {
        match self.hit(point) {
            Hit::Window { window, .. } => {
                if self.windows.focused() != Some(window) && self.windows.focus(window) {
                    self.focus_changed();
                }
            }
            Hit::Chip(i) => {
                // A chip brings its window back into a tile, taking the focused
                // tile's place — the swap the model already knows how to do.
                let Some(id) = self.windows.bar(self.output).get(i).copied() else {
                    return;
                };
                if self.windows.activate(id) {
                    self.focus_changed();
                }
            }
            // The tab slider (M3), the Start Menu (M3), and the empty parts of
            // both bars. Consumed, so a press on system space never reaches an
            // application; nothing to do about it yet.
            Hit::TopBar(_) | Hit::Desktop | Hit::BottomBar => {}
        }
    }

    /// The pointer's position as a logical point.
    fn pointer_point(&self) -> Point {
        Point::new(
            self.pointer_location.x as i32,
            self.pointer_location.y as i32,
        )
    }

    /// What is under a point, asked of core.
    fn hit(&self, point: Point) -> Hit {
        let placed = self.placed_windows();
        hit_test(
            &self.zones(),
            &placed,
            self.wayland.bar_titles(&self.windows).len(),
            point,
        )
    }

    /// The surface under the pointer, and **where that surface is on screen**.
    ///
    /// The second half is the trap this function exists to name. smithay wants
    /// the surface's position in global space and subtracts it itself; handing it
    /// an already-subtracted local position produces `global - local`, which is
    /// the window's own corner — so every client sees the pointer frozen at its
    /// top-left corner and no menu, button or drag ever works, while the keyboard
    /// keeps working perfectly because keys carry no coordinates. Measured and
    /// fixed 2026-08-02; the symptom is documented because it is unmistakable
    /// once seen and baffling until then.
    fn pointer_focus(
        &self,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        SmithayPoint<f64, Logical>,
    )> {
        let Hit::Window { window, .. } = self.hit(self.pointer_point()) else {
            return None;
        };
        let rect = self
            .placed_windows()
            .iter()
            .find(|p| p.window == window)?
            .rect;
        let surface = self.wayland.surface_of(window)?.clone();
        // The surface's origin, not the window's: the renderer skipped the
        // client's shadow margin to put the window on the tile, so the buffer
        // starts that far above and left of what is on screen. Both sides use
        // the same offset or the picture and the clicks disagree.
        let skip = self.buffer_offset(window);
        Some((
            surface,
            ((rect.x() - skip.0) as f64, (rect.y() - skip.1) as f64).into(),
        ))
    }

    fn on_touch_down<B: InputBackend>(&mut self, event: B::TouchDownEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let location = self.absolute::<B, _>(&event);
        let point = Point::new(location.x as i32, location.y as i32);
        let hit = self.hit(point);

        // The shell's own surfaces take direct touch (D-022): a tap on a chip is
        // a tap on a chip, and it does not become a synthetic click on its way
        // there.
        self.press(point);

        let Hit::Window { window, .. } = hit else {
            return;
        };
        let Some(surface) = self.wayland.surface_of(window).cloned() else {
            return;
        };
        let Some(rect) = self
            .placed_windows()
            .iter()
            .find(|p| p.window == window)
            .map(|p| p.rect)
        else {
            return;
        };
        // Surface position and a global location, like the pointer path — the
        // subtraction is smithay's to do (see `pointer_focus`).
        let skip = self.buffer_offset(window);
        let origin: SmithayPoint<f64, Logical> =
            ((rect.x() - skip.0) as f64, (rect.y() - skip.1) as f64).into();
        self.touch_focus = Some((surface.clone(), event.slot()));
        touch.down(
            self,
            Some((surface, origin)),
            &DownEvent {
                slot: event.slot(),
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
            },
        );
    }

    fn on_touch_motion<B: InputBackend>(&mut self, event: B::TouchMotionEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        // A finger that went down on a window keeps that window for as long as it
        // is down, even if it slides off — the protocol has no notion of a touch
        // point changing surface, and neither does a finger dragging a scrollbar.
        let Some((surface, slot)) = self.touch_focus.clone() else {
            return;
        };
        if Some(slot) != Some(event.slot()) {
            return;
        }
        let location = self.absolute::<B, _>(&event);
        let Some(window) = self.wayland.window_of(&surface) else {
            return;
        };
        let Some(rect) = self
            .placed_windows()
            .iter()
            .find(|p| p.window == window)
            .map(|p| p.rect)
        else {
            return;
        };
        let skip = self.buffer_offset(window);
        let origin: SmithayPoint<f64, Logical> =
            ((rect.x() - skip.0) as f64, (rect.y() - skip.1) as f64).into();
        touch.motion(
            self,
            Some((surface, origin)),
            &TouchMotionEvent {
                slot: event.slot(),
                location,
                time: event.time_msec(),
            },
        );
    }

    fn on_touch_up<B: InputBackend>(&mut self, event: B::TouchUpEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        if self.touch_focus.as_ref().map(|(_, slot)| *slot) == Some(event.slot()) {
            self.touch_focus = None;
        }
        touch.up(
            self,
            &UpEvent {
                slot: event.slot(),
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
            },
        );
    }

    fn on_touch_cancel(&mut self) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        self.touch_focus = None;
        touch.cancel(self);
    }
}

/// The touch slot a device event carries, kept next to the surface it went down
/// on.
pub(crate) type TouchGrab = (
    smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    TouchSlot,
);
