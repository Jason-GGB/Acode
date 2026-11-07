//! Global shortcut listener.
//!
//! macOS 27 aborts when rdev turns a key event into text off the main queue
//! (`TISGetInputSourceProperty`). The hotkey detector only needs the virtual
//! key, so the macOS listener reads that and never calls Text Input Source APIs.

/// Blocks on the current thread and forwards key events to `callback`.
pub fn listen(callback: impl FnMut(rdev::Event) + 'static) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::listen(callback)
    }
    #[cfg(not(target_os = "macos"))]
    {
        rdev::listen(callback).map_err(|error| format!("{error:?}"))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::SystemTime;

    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use rdev::{EventType, Key};

    static LAST_FLAGS: AtomicU64 = AtomicU64::new(0);

    pub(super) fn listen(callback: impl FnMut(rdev::Event) + 'static) -> Result<(), String> {
        let callback = Mutex::new(callback);
        core_graphics::event::CGEventTap::with_enabled(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, event_type, event| {
                if let Some(event_type) = event_kind(event_type, event) {
                    if let Ok(mut callback) = callback.lock() {
                        callback(rdev::Event {
                            time: SystemTime::now(),
                            name: None,
                            event_type,
                        });
                    }
                }
                CallbackResult::Keep
            },
            CFRunLoop::run_current,
        )
        .map_err(|_| {
            "macOS refused the event tap; grant Accessibility permission to Acode".to_string()
        })
    }

    fn event_kind(event_type: CGEventType, event: &CGEvent) -> Option<EventType> {
        let code = u16::try_from(event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE))
            .ok()?;
        let key = key_from_code(code);
        match event_type {
            CGEventType::KeyDown => Some(EventType::KeyPress(key)),
            CGEventType::KeyUp => Some(EventType::KeyRelease(key)),
            CGEventType::FlagsChanged => Some(flag_change(key, event.get_flags().bits())),
            _ => None,
        }
    }

    /// Modifier keys arrive as `FlagsChanged`. A smaller flag mask is a release.
    fn flag_change(key: Key, flags: u64) -> EventType {
        let previous = LAST_FLAGS.swap(flags, Ordering::Relaxed);
        if flags < previous {
            EventType::KeyRelease(key)
        } else {
            EventType::KeyPress(key)
        }
    }

    /// macOS virtual key codes, matching rdev 0.5 so the existing detector stays valid.
    fn key_from_code(code: u16) -> Key {
        match code {
            58 => Key::Alt,
            61 => Key::AltGr,
            51 => Key::Backspace,
            57 => Key::CapsLock,
            59 => Key::ControlLeft,
            125 => Key::DownArrow,
            53 => Key::Escape,
            122 => Key::F1,
            109 => Key::F10,
            103 => Key::F11,
            111 => Key::F12,
            120 => Key::F2,
            99 => Key::F3,
            118 => Key::F4,
            96 => Key::F5,
            97 => Key::F6,
            98 => Key::F7,
            100 => Key::F8,
            101 => Key::F9,
            123 => Key::LeftArrow,
            55 => Key::MetaLeft,
            54 => Key::MetaRight,
            36 => Key::Return,
            124 => Key::RightArrow,
            56 => Key::ShiftLeft,
            60 => Key::ShiftRight,
            49 => Key::Space,
            48 => Key::Tab,
            126 => Key::UpArrow,
            50 => Key::BackQuote,
            18 => Key::Num1,
            19 => Key::Num2,
            20 => Key::Num3,
            21 => Key::Num4,
            23 => Key::Num5,
            22 => Key::Num6,
            26 => Key::Num7,
            28 => Key::Num8,
            25 => Key::Num9,
            29 => Key::Num0,
            27 => Key::Minus,
            24 => Key::Equal,
            12 => Key::KeyQ,
            13 => Key::KeyW,
            14 => Key::KeyE,
            15 => Key::KeyR,
            17 => Key::KeyT,
            16 => Key::KeyY,
            32 => Key::KeyU,
            34 => Key::KeyI,
            31 => Key::KeyO,
            35 => Key::KeyP,
            33 => Key::LeftBracket,
            30 => Key::RightBracket,
            0 => Key::KeyA,
            1 => Key::KeyS,
            2 => Key::KeyD,
            3 => Key::KeyF,
            5 => Key::KeyG,
            4 => Key::KeyH,
            38 => Key::KeyJ,
            40 => Key::KeyK,
            37 => Key::KeyL,
            41 => Key::SemiColon,
            39 => Key::Quote,
            42 => Key::BackSlash,
            6 => Key::KeyZ,
            7 => Key::KeyX,
            8 => Key::KeyC,
            9 => Key::KeyV,
            11 => Key::KeyB,
            45 => Key::KeyN,
            46 => Key::KeyM,
            43 => Key::Comma,
            47 => Key::Dot,
            44 => Key::Slash,
            63 => Key::Function,
            other => Key::Unknown(other.into()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{flag_change, key_from_code, LAST_FLAGS};
        use rdev::{EventType, Key};
        use std::sync::atomic::Ordering;

        #[test]
        fn maps_the_default_hotkey_keys() {
            assert_eq!(key_from_code(58), Key::Alt);
            assert_eq!(key_from_code(61), Key::AltGr);
            assert_eq!(key_from_code(59), Key::ControlLeft);
            assert_eq!(key_from_code(56), Key::ShiftLeft);
            assert_eq!(key_from_code(60), Key::ShiftRight);
            assert_eq!(key_from_code(55), Key::MetaLeft);
            assert_eq!(key_from_code(54), Key::MetaRight);
            assert_eq!(key_from_code(49), Key::Space);
            assert_eq!(key_from_code(999), Key::Unknown(999));
        }

        #[test]
        fn shrinking_modifier_mask_is_a_release() {
            LAST_FLAGS.store(0, Ordering::Relaxed);
            assert!(matches!(
                flag_change(Key::Alt, 0x0008_0000),
                EventType::KeyPress(Key::Alt)
            ));
            assert!(matches!(
                flag_change(Key::Alt, 0),
                EventType::KeyRelease(Key::Alt)
            ));
        }
    }
}
