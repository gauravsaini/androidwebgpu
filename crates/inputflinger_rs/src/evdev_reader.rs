//! Linux Evdev Protocol decoder and synthetic virtual event source.

use crate::error::{InputFlingerError, InputFlingerResult};
use crate::types::*;
use input_channel::{KeyEventData, MotionEventData, PointerCoords, PointerProperties, MAX_POINTERS};

// -----------------------------------------------------------------------------
// Linux Evdev Constants
// -----------------------------------------------------------------------------

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;

pub const SYN_REPORT: u16 = 0x00;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_PRESSURE: u16 = 0x18;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const ABS_MT_PRESSURE: u16 = 0x3a;

pub const BTN_TOUCH: u16 = 0x14a;

// -----------------------------------------------------------------------------
// Linux Input Event Structure
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxInputEvent {
    pub time_sec: i64,
    pub time_usec: i64,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

impl LinuxInputEvent {
    pub fn new(type_: u16, code: u16, value: i32) -> Self {
        Self {
            time_sec: 0,
            time_usec: 0,
            type_,
            code,
            value,
        }
    }

    pub fn encode_bytes(&self, buf: &mut [u8]) {
        if buf.len() < 24 {
            return;
        }
        buf[0..8].copy_from_slice(&self.time_sec.to_le_bytes());
        buf[8..16].copy_from_slice(&self.time_usec.to_le_bytes());
        buf[16..18].copy_from_slice(&self.type_.to_le_bytes());
        buf[18..20].copy_from_slice(&self.code.to_le_bytes());
        buf[20..24].copy_from_slice(&self.value.to_le_bytes());
    }

    pub fn decode_bytes(buf: &[u8]) -> InputFlingerResult<Self> {
        if buf.len() < 24 {
            return Err(InputFlingerError::Evdev(format!(
                "Invalid evdev event size: expected 24, got {}",
                buf.len()
            )));
        }
        let time_sec = i64::from_le_bytes(buf[0..8].try_into().unwrap());
        let time_usec = i64::from_le_bytes(buf[8..16].try_into().unwrap());
        let type_ = u16::from_le_bytes(buf[16..18].try_into().unwrap());
        let code = u16::from_le_bytes(buf[18..20].try_into().unwrap());
        let value = i32::from_le_bytes(buf[20..24].try_into().unwrap());

        Ok(Self {
            time_sec,
            time_usec,
            type_,
            code,
            value,
        })
    }
}

// -----------------------------------------------------------------------------
// Touch Slot Tracking
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct TouchSlot {
    active: bool,
    tracking_id: i32,
    x: f32,
    y: f32,
    pressure: f32,
}

// -----------------------------------------------------------------------------
// Evdev Decoder & State Machine
// -----------------------------------------------------------------------------

pub struct EvdevDecoder {
    device_id: i32,
    current_slot: usize,
    slots: [TouchSlot; MAX_POINTERS],
    was_down: bool,
    down_time: i64,
}

impl EvdevDecoder {
    pub fn new(device_id: i32) -> Self {
        Self {
            device_id,
            current_slot: 0,
            slots: [TouchSlot::default(); MAX_POINTERS],
            was_down: false,
            down_time: 0,
        }
    }

    /// Process a stream of raw Linux `LinuxInputEvent`s and return accumulated Android events.
    pub fn process_events(&mut self, events: &[LinuxInputEvent]) -> Vec<InputEvent> {
        let mut result = Vec::new();

        for event in events {
            match event.type_ {
                EV_KEY => {
                    if event.code == BTN_TOUCH {
                        // Handled via tracking ID and slot state
                    } else {
                        let action = if event.value == 1 {
                            KEY_ACTION_DOWN
                        } else {
                            KEY_ACTION_UP
                        };
                        let key_code = Self::linux_to_android_keycode(event.code);
                        let now = event.time_sec * 1_000_000_000 + event.time_usec * 1000;
                        let key_data = KeyEventData {
                            seq: 0,
                            event_time: now,
                            device_id: self.device_id,
                            source: SOURCE_KEYBOARD,
                            display_id: 0,
                            action,
                            flags: 0,
                            key_code,
                            scan_code: event.code as i32,
                            meta_state: 0,
                            repeat_count: if event.value == 2 { 1 } else { 0 },
                            down_time: now,
                        };
                        result.push(InputEvent::Key(key_data));
                    }
                }
                EV_ABS => match event.code {
                    ABS_MT_SLOT => {
                        let slot = event.value as usize;
                        if slot < MAX_POINTERS {
                            self.current_slot = slot;
                        }
                    }
                    ABS_MT_TRACKING_ID => {
                        let id = event.value;
                        if id == -1 {
                            self.slots[self.current_slot].active = false;
                            self.slots[self.current_slot].tracking_id = -1;
                        } else {
                            self.slots[self.current_slot].active = true;
                            self.slots[self.current_slot].tracking_id = id;
                        }
                    }
                    ABS_MT_POSITION_X | ABS_X => {
                        self.slots[self.current_slot].x = event.value as f32;
                    }
                    ABS_MT_POSITION_Y | ABS_Y => {
                        self.slots[self.current_slot].y = event.value as f32;
                    }
                    ABS_MT_PRESSURE | ABS_PRESSURE => {
                        self.slots[self.current_slot].pressure = (event.value as f32) / 255.0;
                    }
                    _ => {}
                },
                EV_SYN => {
                    if event.code == SYN_REPORT {
                        if let Some(motion) = self.build_motion_event(event) {
                            result.push(InputEvent::Motion(motion));
                        }
                    }
                }
                _ => {}
            }
        }

        result
    }

    fn build_motion_event(&mut self, event: &LinuxInputEvent) -> Option<MotionEventData> {
        let mut active_indices = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.active {
                active_indices.push(i);
            }
        }

        let now = event.time_sec * 1_000_000_000 + event.time_usec * 1000;
        let is_down = !active_indices.is_empty();

        let action = match (self.was_down, is_down) {
            (false, true) => {
                self.down_time = now;
                self.was_down = true;
                MOTION_ACTION_DOWN
            }
            (true, true) => MOTION_ACTION_MOVE,
            (true, false) => {
                self.was_down = false;
                MOTION_ACTION_UP
            }
            (false, false) => return None,
        };

        let pointer_count = if is_down {
            active_indices.len() as u32
        } else {
            1
        };

        let mut props = [PointerProperties::default(); MAX_POINTERS];
        let mut coords = [PointerCoords::default(); MAX_POINTERS];

        if is_down {
            for (out_idx, &in_idx) in active_indices.iter().enumerate() {
                let slot = &self.slots[in_idx];
                props[out_idx] = PointerProperties::new(slot.tracking_id, 1);
                coords[out_idx] = PointerCoords::new(slot.x, slot.y, slot.pressure.max(1.0), 1.0);
            }
        } else {
            // Motion up retains last known position
            let slot = &self.slots[self.current_slot];
            props[0] = PointerProperties::new(0, 1);
            coords[0] = PointerCoords::new(slot.x, slot.y, 0.0, 1.0);
        }

        Some(MotionEventData {
            seq: 0,
            event_time: now,
            device_id: self.device_id,
            source: SOURCE_TOUCHSCREEN,
            display_id: 0,
            action,
            action_button: 0,
            flags: 0,
            edge_flags: 0,
            meta_state: 0,
            button_state: 0,
            classification: 0,
            x_precision: 1.0,
            y_precision: 1.0,
            x_offset: 0.0,
            y_offset: 0.0,
            down_time: self.down_time,
            pointer_count,
            pointer_properties: props,
            pointer_coords: coords,
        })
    }

    fn linux_to_android_keycode(linux_code: u16) -> i32 {
        match linux_code {
            1 => KEYCODE_ESCAPE,
            2 => KEYCODE_1,
            3 => KEYCODE_2,
            4 => KEYCODE_3,
            5 => KEYCODE_4,
            6 => KEYCODE_5,
            7 => KEYCODE_6,
            8 => KEYCODE_7,
            9 => KEYCODE_8,
            10 => KEYCODE_9,
            11 => KEYCODE_0,
            28 => KEYCODE_ENTER,
            30 => KEYCODE_A,
            48 => KEYCODE_B,
            46 => KEYCODE_C,
            32 => KEYCODE_D,
            103 => KEYCODE_DPAD_UP,
            108 => KEYCODE_DPAD_DOWN,
            105 => KEYCODE_DPAD_LEFT,
            106 => KEYCODE_DPAD_RIGHT,
            115 => KEYCODE_VOLUME_UP,
            114 => KEYCODE_VOLUME_DOWN,
            116 => KEYCODE_POWER,
            158 => KEYCODE_BACK,
            172 => KEYCODE_HOME,
            _ => KEYCODE_UNKNOWN,
        }
    }
}

// -----------------------------------------------------------------------------
// Virtual Event Source Helper
// -----------------------------------------------------------------------------

pub struct VirtualEventSource {
    device_id: i32,
    down_time: i64,
}

impl VirtualEventSource {
    pub fn new(device_id: i32) -> Self {
        Self {
            device_id,
            down_time: 0,
        }
    }

    pub fn make_touch_down(&mut self, x: f32, y: f32, now: i64) -> MotionEventData {
        self.down_time = now;
        let mut props = [PointerProperties::default(); MAX_POINTERS];
        let mut coords = [PointerCoords::default(); MAX_POINTERS];
        props[0] = PointerProperties::new(0, 1);
        coords[0] = PointerCoords::new(x, y, 1.0, 1.0);

        MotionEventData {
            seq: 0,
            event_time: now,
            device_id: self.device_id,
            source: SOURCE_TOUCHSCREEN,
            display_id: 0,
            action: MOTION_ACTION_DOWN,
            action_button: 0,
            flags: 0,
            edge_flags: 0,
            meta_state: 0,
            button_state: 0,
            classification: 0,
            x_precision: 1.0,
            y_precision: 1.0,
            x_offset: 0.0,
            y_offset: 0.0,
            down_time: now,
            pointer_count: 1,
            pointer_properties: props,
            pointer_coords: coords,
        }
    }

    pub fn make_touch_move(&self, x: f32, y: f32, now: i64) -> MotionEventData {
        let mut props = [PointerProperties::default(); MAX_POINTERS];
        let mut coords = [PointerCoords::default(); MAX_POINTERS];
        props[0] = PointerProperties::new(0, 1);
        coords[0] = PointerCoords::new(x, y, 1.0, 1.0);

        MotionEventData {
            seq: 0,
            event_time: now,
            device_id: self.device_id,
            source: SOURCE_TOUCHSCREEN,
            display_id: 0,
            action: MOTION_ACTION_MOVE,
            action_button: 0,
            flags: 0,
            edge_flags: 0,
            meta_state: 0,
            button_state: 0,
            classification: 0,
            x_precision: 1.0,
            y_precision: 1.0,
            x_offset: 0.0,
            y_offset: 0.0,
            down_time: self.down_time,
            pointer_count: 1,
            pointer_properties: props,
            pointer_coords: coords,
        }
    }

    pub fn make_touch_up(&self, x: f32, y: f32, now: i64) -> MotionEventData {
        let mut props = [PointerProperties::default(); MAX_POINTERS];
        let mut coords = [PointerCoords::default(); MAX_POINTERS];
        props[0] = PointerProperties::new(0, 1);
        coords[0] = PointerCoords::new(x, y, 0.0, 1.0);

        MotionEventData {
            seq: 0,
            event_time: now,
            device_id: self.device_id,
            source: SOURCE_TOUCHSCREEN,
            display_id: 0,
            action: MOTION_ACTION_UP,
            action_button: 0,
            flags: 0,
            edge_flags: 0,
            meta_state: 0,
            button_state: 0,
            classification: 0,
            x_precision: 1.0,
            y_precision: 1.0,
            x_offset: 0.0,
            y_offset: 0.0,
            down_time: self.down_time,
            pointer_count: 1,
            pointer_properties: props,
            pointer_coords: coords,
        }
    }

    pub fn make_key_event(&self, key_code: i32, action: i32, now: i64) -> KeyEventData {
        KeyEventData {
            seq: 0,
            event_time: now,
            device_id: self.device_id,
            source: SOURCE_KEYBOARD,
            display_id: 0,
            action,
            flags: 0,
            key_code,
            scan_code: 0,
            meta_state: 0,
            repeat_count: 0,
            down_time: now,
        }
    }
}
