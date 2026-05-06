pub(crate) fn translate_mouse_button(button: baseview::MouseButton) -> Option<egui::PointerButton> {
    match button {
        baseview::MouseButton::Left => Some(egui::PointerButton::Primary),
        baseview::MouseButton::Right => Some(egui::PointerButton::Secondary),
        baseview::MouseButton::Middle => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

pub(crate) fn translate_virtual_key(key: &keyboard_types::Key) -> Option<egui::Key> {
    use egui::Key;
    use keyboard_types::Key as K;

    Some(match key {
        K::ArrowDown => Key::ArrowDown,
        K::ArrowLeft => Key::ArrowLeft,
        K::ArrowRight => Key::ArrowRight,
        K::ArrowUp => Key::ArrowUp,

        K::Escape => Key::Escape,
        K::Tab => Key::Tab,
        K::Backspace => Key::Backspace,
        K::Enter => Key::Enter,

        K::Insert => Key::Insert,
        K::Delete => Key::Delete,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,

        K::Character(s) => match s.chars().next()? {
            ' ' => Key::Space,
            '0' => Key::Num0,
            '1' => Key::Num1,
            '2' => Key::Num2,
            '3' => Key::Num3,
            '4' => Key::Num4,
            '5' => Key::Num5,
            '6' => Key::Num6,
            '7' => Key::Num7,
            '8' => Key::Num8,
            '9' => Key::Num9,
            'a' => Key::A,
            'b' => Key::B,
            'c' => Key::C,
            'd' => Key::D,
            'e' => Key::E,
            'f' => Key::F,
            'g' => Key::G,
            'h' => Key::H,
            'i' => Key::I,
            'j' => Key::J,
            'k' => Key::K,
            'l' => Key::L,
            'm' => Key::M,
            'n' => Key::N,
            'o' => Key::O,
            'p' => Key::P,
            'q' => Key::Q,
            'r' => Key::R,
            's' => Key::S,
            't' => Key::T,
            'u' => Key::U,
            'v' => Key::V,
            'w' => Key::W,
            'x' => Key::X,
            'y' => Key::Y,
            'z' => Key::Z,
            _ => {
                return None;
            }
        },
        _ => {
            return None;
        }
    })
}

pub(crate) fn translate_cursor_icon(cursor: egui::CursorIcon) -> baseview::MouseCursor {
    match cursor {
        egui::CursorIcon::Default => baseview::MouseCursor::Default,
        egui::CursorIcon::None => baseview::MouseCursor::Hidden,
        egui::CursorIcon::ContextMenu => baseview::MouseCursor::Hand,
        egui::CursorIcon::Help => baseview::MouseCursor::Help,
        egui::CursorIcon::PointingHand => baseview::MouseCursor::Hand,
        egui::CursorIcon::Progress => baseview::MouseCursor::PtrWorking,
        egui::CursorIcon::Wait => baseview::MouseCursor::Working,
        egui::CursorIcon::Cell => baseview::MouseCursor::Cell,
        egui::CursorIcon::Crosshair => baseview::MouseCursor::Crosshair,
        egui::CursorIcon::Text => baseview::MouseCursor::Text,
        egui::CursorIcon::VerticalText => baseview::MouseCursor::VerticalText,
        egui::CursorIcon::Alias => baseview::MouseCursor::Alias,
        egui::CursorIcon::Copy => baseview::MouseCursor::Copy,
        egui::CursorIcon::Move => baseview::MouseCursor::Move,
        egui::CursorIcon::NoDrop => baseview::MouseCursor::NotAllowed,
        egui::CursorIcon::NotAllowed => baseview::MouseCursor::NotAllowed,
        egui::CursorIcon::Grab => baseview::MouseCursor::Hand,
        egui::CursorIcon::Grabbing => baseview::MouseCursor::HandGrabbing,
        egui::CursorIcon::AllScroll => baseview::MouseCursor::AllScroll,
        egui::CursorIcon::ResizeHorizontal => baseview::MouseCursor::EwResize,
        egui::CursorIcon::ResizeNeSw => baseview::MouseCursor::NeswResize,
        egui::CursorIcon::ResizeNwSe => baseview::MouseCursor::NwseResize,
        egui::CursorIcon::ResizeVertical => baseview::MouseCursor::NsResize,
        egui::CursorIcon::ResizeEast => baseview::MouseCursor::EResize,
        egui::CursorIcon::ResizeSouthEast => baseview::MouseCursor::SeResize,
        egui::CursorIcon::ResizeSouth => baseview::MouseCursor::SResize,
        egui::CursorIcon::ResizeSouthWest => baseview::MouseCursor::SwResize,
        egui::CursorIcon::ResizeWest => baseview::MouseCursor::WResize,
        egui::CursorIcon::ResizeNorthWest => baseview::MouseCursor::NwResize,
        egui::CursorIcon::ResizeNorth => baseview::MouseCursor::NResize,
        egui::CursorIcon::ResizeNorthEast => baseview::MouseCursor::NeResize,
        egui::CursorIcon::ResizeColumn => baseview::MouseCursor::ColResize,
        egui::CursorIcon::ResizeRow => baseview::MouseCursor::RowResize,
        egui::CursorIcon::ZoomIn => baseview::MouseCursor::ZoomIn,
        egui::CursorIcon::ZoomOut => baseview::MouseCursor::ZoomOut,
    }
}
