//! InputFlinger data types, device descriptors, event wrappers, and constants.

use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::Parcelable;
use aidl_compat::Parcel;
use input_channel::{InputMessage, KeyEventData, MotionEventData};
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Source Constants
// -----------------------------------------------------------------------------

pub const SOURCE_CLASS_MASK: u32 = 0x000000ff;
pub const SOURCE_CLASS_NONE: u32 = 0x00000000;
pub const SOURCE_CLASS_BUTTON: u32 = 0x00000001;
pub const SOURCE_CLASS_POINTER: u32 = 0x00000002;
pub const SOURCE_CLASS_TRACKBALL: u32 = 0x00000004;
pub const SOURCE_CLASS_POSITION: u32 = 0x00000008;
pub const SOURCE_CLASS_JOYSTICK: u32 = 0x00000010;

pub const SOURCE_KEYBOARD: u32 = 0x00000101;
pub const SOURCE_DPAD: u32 = 0x00000201;
pub const SOURCE_GAMEPAD: u32 = 0x00000401;
pub const SOURCE_TOUCHSCREEN: u32 = 0x00001002;
pub const SOURCE_MOUSE: u32 = 0x00002002;
pub const SOURCE_STYLUS: u32 = 0x00004002;
pub const SOURCE_TRACKBALL: u32 = 0x00000004;
pub const SOURCE_TOUCHPAD: u32 = 0x00100008;
pub const SOURCE_ANY: u32 = 0xffffffff;

// -----------------------------------------------------------------------------
// Key Code Constants
// -----------------------------------------------------------------------------

pub const KEYCODE_UNKNOWN: i32 = 0;
pub const KEYCODE_HOME: i32 = 3;
pub const KEYCODE_BACK: i32 = 4;
pub const KEYCODE_CALL: i32 = 5;
pub const KEYCODE_ENDCALL: i32 = 6;
pub const KEYCODE_0: i32 = 7;
pub const KEYCODE_1: i32 = 8;
pub const KEYCODE_2: i32 = 9;
pub const KEYCODE_3: i32 = 10;
pub const KEYCODE_4: i32 = 11;
pub const KEYCODE_5: i32 = 12;
pub const KEYCODE_6: i32 = 13;
pub const KEYCODE_7: i32 = 14;
pub const KEYCODE_8: i32 = 15;
pub const KEYCODE_9: i32 = 16;
pub const KEYCODE_DPAD_UP: i32 = 19;
pub const KEYCODE_DPAD_DOWN: i32 = 20;
pub const KEYCODE_DPAD_LEFT: i32 = 21;
pub const KEYCODE_DPAD_RIGHT: i32 = 22;
pub const KEYCODE_DPAD_CENTER: i32 = 23;
pub const KEYCODE_VOLUME_UP: i32 = 24;
pub const KEYCODE_VOLUME_DOWN: i32 = 25;
pub const KEYCODE_POWER: i32 = 26;
pub const KEYCODE_CAMERA: i32 = 27;
pub const KEYCODE_CLEAR: i32 = 28;
pub const KEYCODE_A: i32 = 29;
pub const KEYCODE_B: i32 = 30;
pub const KEYCODE_C: i32 = 31;
pub const KEYCODE_D: i32 = 32;
pub const KEYCODE_ENTER: i32 = 66;
pub const KEYCODE_ESCAPE: i32 = 111;

// -----------------------------------------------------------------------------
// Action Constants
// -----------------------------------------------------------------------------

pub const KEY_ACTION_DOWN: i32 = 0;
pub const KEY_ACTION_UP: i32 = 1;
pub const KEY_ACTION_MULTIPLE: i32 = 2;

pub const MOTION_ACTION_DOWN: i32 = 0;
pub const MOTION_ACTION_UP: i32 = 1;
pub const MOTION_ACTION_MOVE: i32 = 2;
pub const MOTION_ACTION_CANCEL: i32 = 3;
pub const MOTION_ACTION_OUTSIDE: i32 = 4;
pub const MOTION_ACTION_POINTER_DOWN: i32 = 5;
pub const MOTION_ACTION_POINTER_UP: i32 = 6;
pub const MOTION_ACTION_HOVER_MOVE: i32 = 7;
pub const MOTION_ACTION_SCROLL: i32 = 8;

// -----------------------------------------------------------------------------
// Injection Mode
// -----------------------------------------------------------------------------

pub const INJECT_INPUT_EVENT_MODE_ASYNC: i32 = 0;
pub const INJECT_INPUT_EVENT_MODE_WAIT_FOR_RESULT: i32 = 1;
pub const INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH: i32 = 2;

// -----------------------------------------------------------------------------
// InputDevice
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputDevice {
    pub id: i32,
    pub generation: i32,
    pub name: String,
    pub sources: u32,
    pub keyboard_type: i32,
    pub has_vibrator: bool,
    pub has_mic: bool,
}

impl Default for InputDevice {
    fn default() -> Self {
        Self {
            id: 1,
            generation: 1,
            name: "Virtual Touchscreen".to_string(),
            sources: SOURCE_TOUCHSCREEN,
            keyboard_type: 0,
            has_vibrator: false,
            has_mic: false,
        }
    }
}

impl InputDevice {
    pub fn new_touchscreen(id: i32, name: impl Into<String>) -> Self {
        Self {
            id,
            generation: 1,
            name: name.into(),
            sources: SOURCE_TOUCHSCREEN,
            keyboard_type: 0,
            has_vibrator: false,
            has_mic: false,
        }
    }

    pub fn new_keyboard(id: i32, name: impl Into<String>) -> Self {
        Self {
            id,
            generation: 1,
            name: name.into(),
            sources: SOURCE_KEYBOARD,
            keyboard_type: 2, // ALPHABETIC
            has_vibrator: false,
            has_mic: false,
        }
    }
}

impl Parcelable for InputDevice {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.generation).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.name)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.sources).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.keyboard_type).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_bool(self.has_vibrator).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_bool(self.has_mic).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.id = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.generation = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.name = parcel.read_utf8(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.sources = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.keyboard_type = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.has_vibrator = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.has_mic = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// InputEvent
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum InputEvent {
    Key(KeyEventData),
    Motion(MotionEventData),
}

impl InputEvent {
    pub fn to_input_message(&self) -> InputMessage {
        match self {
            InputEvent::Key(k) => InputMessage::Key(k.clone()),
            InputEvent::Motion(m) => InputMessage::Motion(m.clone()),
        }
    }
}

impl Default for InputEvent {
    fn default() -> Self {
        InputEvent::Key(KeyEventData::default())
    }
}

impl Parcelable for InputEvent {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        match self {
            InputEvent::Key(k) => {
                parcel.write_i32(1).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut buf = [0u8; 56];
                k.encode_bytes(&mut buf);
                parcel.write_byte_slice(Some(&buf)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            }
            InputEvent::Motion(m) => {
                parcel.write_i32(2).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut buf = [0u8; 1024];
                m.encode_bytes(&mut buf);
                parcel.write_byte_slice(Some(&buf)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            }
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let tag = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let bytes = parcel.read_byte_vec(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;

        match tag {
            1 => {
                let k = KeyEventData::decode_bytes(&bytes).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                *self = InputEvent::Key(k);
            }
            2 => {
                let m = MotionEventData::decode_bytes(&bytes).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                *self = InputEvent::Motion(m);
            }
            _ => return Err(Status::from_status(STATUS_BAD_VALUE)),
        }
        Ok(())
    }
}
