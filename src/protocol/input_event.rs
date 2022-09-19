use serde::{Deserialize, Serialize};
use strum::{EnumIter, FromRepr};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum InputEvent {
    MouseMove { dx: i32, dy: i32 },

    MouseButtonDown { button: MouseButton },
    MouseButtonUp { button: MouseButton },

    MouseScroll {},

    KeyDown { key: KeyCode },
    KeyRepeat { key: KeyCode },
    KeyUp { key: KeyCode },
}

#[repr(u8)]
#[derive(FromRepr, EnumIter, Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum MouseButton {
    Left = 0,
    Right,
    Middle,
    Mouse4,
    Mouse5,
}

/// Keyboard key.
#[repr(u16)]
#[derive(FromRepr, EnumIter, Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum KeyCode {
    Escape = 0,

    // function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    PrintScreen,
    ScrollLock,
    PauseBreak,

    /// The tilde key.
    Grave,

    // digits
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D0,

    Minus,
    Equal,

    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    LeftBrace,
    RightBrace,

    SemiColon,
    Apostrophe,

    Comma,
    Dot,
    Slash,

    Backspace,
    BackSlash,
    Enter,

    Space,

    Tab,
    CapsLock,

    LeftShift,
    RightShift,

    LeftCtrl,
    RightCtrl,

    LeftAlt,
    RightAlt,

    LeftMeta,
    RightMeta,

    Insert,
    Delete,

    Home,
    End,

    PageUp,
    PageDown,

    Up,
    Left,
    Down,
    Right,
}

/// Define a bidirectional conversion.
macro_rules! define_conversion {
    ($l_ty:ty, $r_ty:ty, { $($l_var:ident => $r_var:ident,)* }) => {
        impl Into<$r_ty> for $l_ty {
            fn into(self) -> $r_ty {
                use $l_ty::*;
                use $r_ty::*;
                match self {
                    $($l_var => $r_var,)*
                }
            }
        }

        paste::paste! {
            impl $l_ty {
                pub fn [<from_$r_ty:lower>](x: $r_ty) -> Option<Self> {
                    use $r_ty::*;
                    use $l_ty::*;
                    match x {
                        $($r_var => Some($l_var),)*
                        _ => None,
                    }
                }
            }
        }
    };

    (
        $l_ty:ty,
        $r_ty:ty,
        {
            $($l_var:ident = $r_var:expr,)*
        }
    ) => {
        impl Into<$r_ty> for $l_ty {
            fn into(self) -> $r_ty {
                use $l_ty::*;
                match self {
                    $($l_var => $r_var,)*
                }
            }
        }

        paste::paste! {
            impl $l_ty {
                pub fn [<from_$r_ty:lower>](x: $r_ty) -> Option<Self> {
                    use $l_ty::*;
                    match x {
                        $(x if x == $r_var => Some($l_var),)*
                        _ => None,
                    }
                }
            }
        }
    };
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use evdev_rs::enums::EV_KEY;

    define_conversion!(KeyCode, EV_KEY, {
        Escape => KEY_ESC,

        F1 => KEY_F1,
        F2 => KEY_F2,
        F3 => KEY_F3,
        F4 => KEY_F4,
        F5 => KEY_F5,
        F6 => KEY_F6,
        F7 => KEY_F7,
        F8 => KEY_F8,
        F9 => KEY_F9,
        F10 => KEY_F10,
        F11 => KEY_F11,
        F12 => KEY_F12,

        PrintScreen => KEY_PRINT,
        ScrollLock => KEY_SCROLLLOCK,
        PauseBreak => KEY_PAUSE,

        Grave => KEY_GRAVE,

        D1 => KEY_1,
        D2 => KEY_2,
        D3 => KEY_3,
        D4 => KEY_4,
        D5 => KEY_5,
        D6 => KEY_6,
        D7 => KEY_7,
        D8 => KEY_8,
        D9 => KEY_9,
        D0 => KEY_0,

        Minus => KEY_MINUS,
        Equal => KEY_EQUAL,

        A => KEY_A,
        B => KEY_B,
        C => KEY_C,
        D => KEY_D,
        E => KEY_E,
        F => KEY_F,
        G => KEY_G,
        H => KEY_H,
        I => KEY_I,
        J => KEY_J,
        K => KEY_K,
        L => KEY_L,
        M => KEY_M,
        N => KEY_N,
        O => KEY_O,
        P => KEY_P,
        Q => KEY_Q,
        R => KEY_R,
        S => KEY_S,
        T => KEY_T,
        U => KEY_U,
        V => KEY_V,
        W => KEY_W,
        X => KEY_X,
        Y => KEY_Y,
        Z => KEY_Z,

        LeftBrace => KEY_LEFTBRACE,
        RightBrace => KEY_RIGHTBRACE,

        SemiColon => KEY_SEMICOLON,
        Apostrophe => KEY_APOSTROPHE,

        Comma => KEY_COMMA,
        Dot => KEY_DOT,
        Slash => KEY_SLASH,

        Backspace => KEY_BACKSPACE,
        BackSlash => KEY_BACKSLASH,
        Enter => KEY_ENTER,

        Space => KEY_SPACE,

        Tab => KEY_TAB,
        CapsLock => KEY_CAPSLOCK,

        LeftShift => KEY_LEFTSHIFT,
        RightShift => KEY_RIGHTSHIFT,

        LeftCtrl => KEY_LEFTCTRL,
        RightCtrl => KEY_RIGHTCTRL,

        LeftAlt => KEY_LEFTALT,
        RightAlt => KEY_RIGHTALT,

        LeftMeta => KEY_LEFTMETA,
        RightMeta => KEY_RIGHTMETA,

        Insert => KEY_INSERT,
        Delete => KEY_DELETE,

        Home => KEY_HOME,
        End => KEY_END,

        PageUp => KEY_PAGEUP,
        PageDown => KEY_PAGEDOWN,

        Up => KEY_UP,
        Left => KEY_LEFT,
        Down => KEY_DOWN,
        Right => KEY_RIGHT,
    });

    define_conversion!(MouseButton, EV_KEY, {
        Left => BTN_LEFT,
        Right => BTN_RIGHT,
        Middle => BTN_MIDDLE,
        Mouse4 => BTN_4,
        Mouse5 => BTN_5,
    });
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::UI::Input::KeyboardAndMouse::*;

    // https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes
    define_conversion!(KeyCode, u16, {
        Escape = VK_ESCAPE.0,

        F1 = VK_F1.0,
        F2 = VK_F2.0,
        F3 = VK_F3.0,
        F4 = VK_F4.0,
        F5 = VK_F5.0,
        F6 = VK_F6.0,
        F7 = VK_F7.0,
        F8 = VK_F8.0,
        F9 = VK_F9.0,
        F10 = VK_F10.0,
        F11 = VK_F11.0,
        F12 = VK_F12.0,

        PrintScreen = VK_SNAPSHOT.0,
        ScrollLock = VK_SCROLL.0,
        PauseBreak = VK_PAUSE.0,

        Grave = VK_OEM_3.0,

        D1 = 0x31,
        D2 = 0x32,
        D3 = 0x33,
        D4 = 0x34,
        D5 = 0x35,
        D6 = 0x36,
        D7 = 0x37,
        D8 = 0x38,
        D9 = 0x39,
        D0 = 0x30,

        Minus = VK_OEM_MINUS.0,
        Equal = VK_OEM_PLUS.0,

        A = 0x41,
        B = 0x42,
        C = 0x43,
        D = 0x44,
        E = 0x45,
        F = 0x46,
        G = 0x47,
        H = 0x48,
        I = 0x49,
        J = 0x4A,
        K = 0x4B,
        L = 0x4C,
        M = 0x4D,
        N = 0x4E,
        O = 0x4F,
        P = 0x50,
        Q = 0x51,
        R = 0x52,
        S = 0x53,
        T = 0x54,
        U = 0x55,
        V = 0x56,
        W = 0x57,
        X = 0x58,
        Y = 0x59,
        Z = 0x5A,

        LeftBrace = VK_OEM_4.0,
        RightBrace = VK_OEM_6.0,

        SemiColon = VK_OEM_1.0,
        Apostrophe = VK_OEM_7.0,

        Comma = VK_OEM_COMMA.0,
        Dot = VK_OEM_PERIOD.0,
        Slash = VK_OEM_2.0,

        Backspace = VK_BACK.0,
        BackSlash = VK_OEM_5.0,
        Enter = VK_RETURN.0,

        Space = VK_SPACE.0,

        Tab = VK_TAB.0,
        CapsLock = VK_CAPITAL.0,

        LeftShift = VK_LSHIFT.0,
        RightShift = VK_RSHIFT.0,

        LeftCtrl = VK_LCONTROL.0,
        RightCtrl = VK_RCONTROL.0,

        LeftAlt = VK_LMENU.0,
        RightAlt = VK_RMENU.0,

        LeftMeta = VK_LWIN.0,
        RightMeta = VK_RWIN.0,

        Insert = VK_INSERT.0,
        Delete = VK_DELETE.0,

        Home = VK_HOME.0,
        End = VK_END.0,

        PageUp = VK_PRIOR.0,
        PageDown = VK_NEXT.0,

        Up = VK_UP.0,
        Left = VK_LEFT.0,
        Down = VK_DOWN.0,
        Right = VK_RIGHT.0,
    });
}
