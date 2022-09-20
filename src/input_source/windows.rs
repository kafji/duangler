use super::event::{LocalInputEvent, MousePosition};
use crate::{
    input_source::controller::InputController,
    protocol::{windows::VirtualKey, InputEvent, KeyCode, MouseButton, MouseScrollDirection},
};
use once_cell::sync::OnceCell;
use std::{
    cmp,
    ffi::c_void,
    sync::atomic::{self, AtomicBool},
};
use tokio::{sync::mpsc, task};
use tracing::{debug, error, warn};
use windows::Win32::{
    Foundation::{GetLastError, LPARAM, LRESULT, RECT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostMessageW, SetCursorPos,
        SetWindowsHookExW, SystemParametersInfoW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
        KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        WHEEL_DELTA, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

/// RAII for unhooking hook.
///
/// Calls [UnhookWindowsHookEx] on drop.
struct Unhooker(HHOOK);

impl Drop for Unhooker {
    fn drop(&mut self) {
        let ok: bool = unsafe { UnhookWindowsHookEx(self.0) }.into();
        if !ok {
            error!("failed to unhook {:?}", self.0);
        }
    }
}

/// This function leaks its state globally because of that it might panic if called multiple time.
pub fn start(event_tx: mpsc::Sender<InputEvent>) -> task::JoinHandle<()> {
    task::spawn_blocking(|| run_input_source(event_tx))
}

/// Application defined message code.
///
/// https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-app
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Debug)]
enum MessageCode {
    InputEvent = WM_APP,
}

static CURSOR_LOCKED_POS: OnceCell<MousePosition> = OnceCell::new();

fn cursor_locked_pos() -> MousePosition {
    *CURSOR_LOCKED_POS
        .get()
        .expect("cursor locked pos was empty")
}

fn run_input_source(event_tx: mpsc::Sender<InputEvent>) {
    let mut controller = InputController::new(event_tx);

    unsafe {
        let mut rect = RECT::default();
        let ptr_rect = &mut rect as *mut _ as *mut c_void;
        let b = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            ptr_rect,
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS::default(),
        );
        assert!(b == true);
        let x = (rect.right / 2) as _;
        let y = (rect.bottom / 2) as _;
        CURSOR_LOCKED_POS
            .set(MousePosition { x, y })
            .expect("failed to set cursor locked position");
    }

    // get module handle for this application
    let module = unsafe { GetModuleHandleW(None) }.expect("failed to get current module handle");
    assert!(!module.is_invalid());

    // set low level mouse hook
    let _mouse_hook = Unhooker(
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), module, 0) }
            .expect("failed to set mouse hook"),
    );

    // set low level keyboard hook
    let _keyboard_hook = Unhooker(
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), module, 0) }
            .expect("failed to set keyboard hook"),
    );

    let mut previous_event = None;

    loop {
        // set cursor position to its locked position if we're capturing input
        if capture_input() {
            let MousePosition { x, y } = cursor_locked_pos();
            unsafe { SetCursorPos(x as _, y as _) };
        }

        let mut msg = MSG::default();
        let ok = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match ok.0 {
            -1 => unsafe {
                let err = GetLastError();
                error!("get message error, {:?}", err);
                break;
            },
            0 => {
                debug!("received quit message");
                break;
            }
            _ => {
                match msg.message {
                    WM_QUIT => {
                        debug!("received quit message");
                        break;
                    }
                    n if n == MessageCode::InputEvent as _ => {
                        // get pointer to input event from lparam
                        let ptr_event = msg.lParam.0 as *mut LocalInputEvent;
                        // acquire input event, the box will ensure it will be freed
                        let event = *unsafe { Box::from_raw(ptr_event) };

                        let event2 = match (previous_event, &event) {
                            (
                                Some(LocalInputEvent::KeyDown { key: prev_key }),
                                LocalInputEvent::KeyDown { key },
                            ) if prev_key == *key => LocalInputEvent::KeyRepeat { key: prev_key },
                            _ => event,
                        };

                        previous_event = Some(event);

                        // propagate input event to the sink
                        let capture_input = controller.on_input_event(event2).unwrap();
                        set_capture_input(capture_input);
                    }
                    _ => unsafe {
                        DispatchMessageW(&msg);
                    },
                }
            }
        }
    }
}

/// If the hooks should consume user inputs.
static CAPTURE_INPUT: AtomicBool = AtomicBool::new(false);

fn capture_input() -> bool {
    CAPTURE_INPUT.load(atomic::Ordering::SeqCst)
}

fn set_capture_input(value: bool) {
    debug!(?value, "set capture input flag");
    CAPTURE_INPUT.store(value, atomic::Ordering::SeqCst)
}

/// Procedure for low level mouse hook.
extern "system" fn mouse_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // per documentation, ncode will always be HC_ACTION
    assert_eq!(ncode, HC_ACTION as _);

    // pointer dance to get MSLLHOOKSTRUCT from lparam
    let ptr_hook_event = lparam.0 as *const MSLLHOOKSTRUCT;
    let hook_event = unsafe { *ptr_hook_event };

    // map hook event to input event
    let event = match wparam.0 as u32 {
        WM_MOUSEMOVE => {
            let x = hook_event.pt.x as _;
            let y = hook_event.pt.y as _;
            let pos = MousePosition { x, y };

            if capture_input() {
                let cpos = cursor_locked_pos();
                let mvment = cpos.delta_to(&pos);
                LocalInputEvent::MouseMove(mvment)
            } else {
                LocalInputEvent::MousePosition(pos)
            }
            .into()
        }

        WM_LBUTTONDOWN => LocalInputEvent::MouseButtonDown {
            button: MouseButton::Left,
        }
        .into(),

        WM_LBUTTONUP => LocalInputEvent::MouseButtonUp {
            button: MouseButton::Left,
        }
        .into(),

        WM_RBUTTONDOWN => LocalInputEvent::MouseButtonDown {
            button: MouseButton::Right,
        }
        .into(),

        WM_RBUTTONUP => LocalInputEvent::MouseButtonUp {
            button: MouseButton::Right,
        }
        .into(),

        WM_MOUSEWHEEL => {
            let delta = {
                let mut bytes = [0; 2];
                bytes.copy_from_slice(&hook_event.mouseData.0.to_be_bytes()[..2]);
                i16::from_be_bytes(bytes)
            };
            let delta = delta / WHEEL_DELTA as i16;
            let direction = match delta.cmp(&0) {
                cmp::Ordering::Less => MouseScrollDirection::Down {
                    clicks: delta.abs() as _,
                },
                cmp::Ordering::Equal => unimplemented!(),
                cmp::Ordering::Greater => MouseScrollDirection::Up {
                    clicks: delta.abs() as _,
                },
            };
            LocalInputEvent::MouseScroll { direction }
        }
        .into(),

        action => {
            debug!(?action, "unhandled mouse event");
            None
        }
    };

    if let Some(event) = event {
        let consume = propagate_input_event(event);
        if consume {
            return LRESULT(1);
        }
    }

    // passthrough
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

/// Procedure for low level keyboard hook.
extern "system" fn keyboard_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // per documentation, ncode will always be HC_ACTION
    assert_eq!(ncode, HC_ACTION as _);

    // pointer dance to get KBDLLHOOKSTRUCT from lparam
    let ptr_hook_event = lparam.0 as *const KBDLLHOOKSTRUCT;
    let hook_event = unsafe { *ptr_hook_event };

    // map hook event to input event
    let key = KeyCode::from_virtual_key(VirtualKey(hook_event.vkCode as _)).unwrap();
    let event = match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => LocalInputEvent::KeyDown { key }.into(),

        WM_KEYUP | WM_SYSKEYUP => LocalInputEvent::KeyUp { key }.into(),

        action => {
            warn!(?action, "unhandled keyboard event");
            None
        }
    };

    if let Some(event) = event {
        let consume = propagate_input_event(event);
        if consume {
            return LRESULT(1);
        }
    }

    // passthrough
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

/// Send input event to the message queue.
///
/// Retruns `true` if event should be consumed, `false` if event should be forwarded to the next hook.
fn propagate_input_event(event: LocalInputEvent) -> bool {
    let event = {
        let x = Box::new(event);
        Box::leak(x)
    };
    let ptr_event = event as *mut _;

    unsafe {
        let b = PostMessageW(
            None,
            MessageCode::InputEvent as _,
            WPARAM::default(),
            LPARAM(ptr_event as isize),
        );
        let b: bool = b.into();
        assert_eq!(b, true);
    }

    // if should capture, consume the event instead of passing it through
    capture_input()
}
