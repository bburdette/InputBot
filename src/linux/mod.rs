extern crate x11;

use self::x11::xlib::*;
use self::x11::xtest::*;
use std::mem::uninitialized;
use std::ptr::null;
use ::*;

mod inputs;
pub use self::inputs::*;

type KeyCodesMap = HashMap<u64, KeybdKey>;
type ButtonStatesMap = HashMap<MouseButton, bool>;

lazy_static! {
    static ref KEYCODES_TO_KEYBDKEYS: Mutex<KeyCodesMap> = Mutex::new(KeyCodesMap::new());
    static ref BUTTON_STATES: Mutex<ButtonStatesMap> = Mutex::new(ButtonStatesMap::new());
    static ref SEND_DISPLAY: AtomicPtr<Display> = {
        unsafe { XInitThreads() };
        AtomicPtr::new(unsafe { XOpenDisplay(null()) })
    };
    static ref RECV_DISPLAY: AtomicPtr<Display> = {
        unsafe { XInitThreads() };
        AtomicPtr::new(unsafe { XOpenDisplay(null()) })
    };
}

impl KeybdKey {
    pub fn is_pressed(self) -> bool {
        let code = get_key_code(u64::from(self) as _);
        let mut array: [i8; 32] = [0; 32];
        SEND_DISPLAY.with(|display| unsafe {
            XQueryKeymap(display, &mut array as *mut [i8; 32] as *mut i8);
        });
        array[(code >> 3) as usize] & (1 << (code & 7)) != 0
    }

    pub fn press(self) {
        send_keybd_input(get_key_code(u64::from(self) as _), 1);
    }

    pub fn release(self) {
        send_keybd_input(get_key_code(u64::from(self) as _), 0);
    }

    pub fn is_toggled(self) -> bool {
        if let Some(key) = match self {
            KeybdKey::NumLockKey => Some(2),
            KeybdKey::CapsLockKey => Some(1),
            _ => None,
        } {
            let mut state: XKeyboardState = unsafe { uninitialized() };
            SEND_DISPLAY.with(|display| unsafe {
                XGetKeyboardControl(display, &mut state);
            });
            (state.led_mask & key != 0)
        } else {
            false
        }
    }
}

impl MouseButton {
    pub fn is_pressed(self) -> bool {
        *BUTTON_STATES.lock().unwrap().entry(self).or_insert(false)
    }

    pub fn press(self) {
        send_mouse_input(u32::from(self), 1);
    }

    pub fn release(self) {
        send_mouse_input(u32::from(self), 0);
    }
}

impl MouseCursor {
    pub fn move_rel(self, x: i32, y: i32) {
        SEND_DISPLAY.with(|display| unsafe {
            XWarpPointer(display, 0, 0, 0, 0, 0, 0, x, y);
        });
    }

    pub fn move_abs(self, x: i32, y: i32) {
        SEND_DISPLAY.with(|display| unsafe {
            XWarpPointer(display, 0, 0, 0, 0, 0, 0, x, y);
        });
    }
}

impl MouseWheel {
    pub fn scroll_ver(self, y: i32) {
        if y < 0 {
          MouseButton::OtherButton(4).press();
          MouseButton::OtherButton(4).release();
        } else {
          MouseButton::OtherButton(5).press();
          MouseButton::OtherButton(5).release();
        }
    }
    pub fn scroll_hor(self, x: i32) {
        if x < 0 {
          MouseButton::OtherButton(6).press();
          MouseButton::OtherButton(6).release();
        } else {
          MouseButton::OtherButton(7).press();
          MouseButton::OtherButton(7).release();
        }
    }
}

pub fn handle_input_events() {
    RECV_DISPLAY.with(|display| {
        let window = unsafe { XDefaultRootWindow(display) };
        for (button, _) in MOUSE_BINDS.lock().unwrap().iter() {
            grab_button(u32::from(*button), display, window);
        }
        for (key, _) in KEYBD_BINDS.lock().unwrap().iter() {
            let key_code = u64::from(get_key_code(u64::from(*key)));
            KEYCODES_TO_KEYBDKEYS.lock().unwrap().insert(key_code, *key);
            grab_key(key_code as i32, AnyModifier, display, window);
        }
    });
    while !MOUSE_BINDS.lock().unwrap().is_empty() || !KEYBD_BINDS.lock().unwrap().is_empty() {
        handle_input_event();
    }
}

fn handle_input_event() {
    let mut ev = unsafe { uninitialized() };
    RECV_DISPLAY.with(|display| unsafe { XNextEvent(display, &mut ev) });
    match ev.get_type() {
        2 => if let Some(keybd_key) = KEYCODES_TO_KEYBDKEYS
            .lock()
            .unwrap()
            .get_mut(&u64::from((ev.as_ref() as &XKeyEvent).keycode))
        {
            if let Some(cb) = KEYBD_BINDS.lock().unwrap().get(keybd_key) {
                let cb = Arc::clone(cb);
                spawn(move || cb());
            };
        },
        4 => {
            let mouse_button = MouseButton::from((ev.as_ref() as &XKeyEvent).keycode);
            BUTTON_STATES.lock().unwrap().insert(mouse_button, true);
            if let Some(cb) = MOUSE_BINDS.lock().unwrap().get(&mouse_button) {
                let cb = Arc::clone(cb);
                spawn(move || cb());
            };
        }
        5 => {
            BUTTON_STATES.lock().unwrap().insert(
                MouseButton::from((ev.as_ref() as &XKeyEvent).keycode),
                false,
            );
        }
        _ => {}
    };
}

fn get_key_code(code: u64) -> u8 {
    SEND_DISPLAY.with(|display| unsafe { XKeysymToKeycode(display, code) })
}

fn grab_button(button: u32, display: *mut Display, window: u64) {
    unsafe {
        XGrabButton(
            display,
            button,
            AnyModifier,
            window,
            1,
            (ButtonPressMask | ButtonReleaseMask) as u32,
            GrabModeAsync,
            GrabModeAsync,
            0,
            0,
        );
    }
}

fn grab_key(key: i32, mask: u32, display: *mut Display, window: u64) {
    unsafe {
        XGrabKey(display, key, mask, window, 0, GrabModeAsync, GrabModeAsync);
    }
}

fn send_keybd_input(code: u8, is_press: i32) {
    SEND_DISPLAY.with(|display| unsafe {
        XTestFakeKeyEvent(display, u32::from(code), is_press, 0);
    });
}

fn send_mouse_input(button: u32, is_press: i32) {
    SEND_DISPLAY.with(|display| unsafe {
        XTestFakeButtonEvent(display, button, is_press, 0);
    });
}

trait DisplayAcquirable {
    fn with<F, Z>(&self, cb: F) -> Z
    where
        F: FnOnce(*mut Display) -> Z;
}

impl DisplayAcquirable for AtomicPtr<Display> {
    fn with<F, Z>(&self, cb: F) -> Z
    where
        F: FnOnce(*mut Display) -> Z,
    {
        let display = self.load(Ordering::Relaxed);
        unsafe {
            XLockDisplay(display);
        };
        let cb_result = cb(display);
        unsafe {
            XFlush(display);
            XUnlockDisplay(display);
        };
        cb_result
    }
}
