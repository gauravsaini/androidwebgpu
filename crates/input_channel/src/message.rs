//! Android 13 `InputMessage` wire format codec and data structures.

use crate::error::{InputChannelError, Result};
use aidl_compat::{Parcel, Parcelable, Result as AidlResult, Status, STATUS_BAD_VALUE};
use serde::{Deserialize, Serialize};

/// Android 13 InputMessage type identifiers.
pub const INPUT_MESSAGE_TYPE_KEY: u32 = 1;
pub const INPUT_MESSAGE_TYPE_MOTION: u32 = 2;
pub const INPUT_MESSAGE_TYPE_FINISHED: u32 = 3;
pub const INPUT_MESSAGE_TYPE_FOCUS: u32 = 4;
pub const INPUT_MESSAGE_TYPE_CAPTURE: u32 = 5;
pub const INPUT_MESSAGE_TYPE_DRAG: u32 = 6;

/// Maximum number of pointers supported in an Android MotionEvent.
pub const MAX_POINTERS: usize = 16;

/// Minimum header size for binary InputMessage wire format (type: u32, seq: u32).
pub const INPUT_MESSAGE_HEADER_SIZE: usize = 8;

/// Fixed binary wire size for Android 13 InputMessage (aligned to 1024 bytes for socketpair transport).
pub const INPUT_MESSAGE_WIRE_SIZE: usize = 1024;

// -----------------------------------------------------------------------------
// Pointer Coordinates and Properties
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerCoords {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub size: f32,
    pub touch_major: f32,
    pub touch_minor: f32,
    pub tool_major: f32,
    pub tool_minor: f32,
    pub orientation: f32,
    pub distance: f32,
    pub is_resampled: bool,
}

impl Default for PointerCoords {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
            size: 1.0,
            touch_major: 0.0,
            touch_minor: 0.0,
            tool_major: 0.0,
            tool_minor: 0.0,
            orientation: 0.0,
            distance: 0.0,
            is_resampled: false,
        }
    }
}

impl PointerCoords {
    pub fn new(x: f32, y: f32, pressure: f32, size: f32) -> Self {
        Self {
            x,
            y,
            pressure,
            size,
            ..Default::default()
        }
    }

    pub fn encode_bytes(&self, buf: &mut [u8]) {
        if buf.len() < 44 {
            return;
        }
        buf[0..4].copy_from_slice(&self.x.to_le_bytes());
        buf[4..8].copy_from_slice(&self.y.to_le_bytes());
        buf[8..12].copy_from_slice(&self.pressure.to_le_bytes());
        buf[12..16].copy_from_slice(&self.size.to_le_bytes());
        buf[16..20].copy_from_slice(&self.touch_major.to_le_bytes());
        buf[20..24].copy_from_slice(&self.touch_minor.to_le_bytes());
        buf[24..28].copy_from_slice(&self.tool_major.to_le_bytes());
        buf[28..32].copy_from_slice(&self.tool_minor.to_le_bytes());
        buf[32..36].copy_from_slice(&self.orientation.to_le_bytes());
        buf[36..40].copy_from_slice(&self.distance.to_le_bytes());
        buf[40..44].copy_from_slice(&(if self.is_resampled { 1u32 } else { 0u32 }).to_le_bytes());
    }

    pub fn decode_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 44 {
            return Err(InputChannelError::InvalidMessageSize {
                expected: 44,
                actual: buf.len(),
            });
        }
        let x = f32::from_le_bytes(buf[0..4].try_into().unwrap());
        let y = f32::from_le_bytes(buf[4..8].try_into().unwrap());
        let pressure = f32::from_le_bytes(buf[8..12].try_into().unwrap());
        let size = f32::from_le_bytes(buf[12..16].try_into().unwrap());
        let touch_major = f32::from_le_bytes(buf[16..20].try_into().unwrap());
        let touch_minor = f32::from_le_bytes(buf[20..24].try_into().unwrap());
        let tool_major = f32::from_le_bytes(buf[24..28].try_into().unwrap());
        let tool_minor = f32::from_le_bytes(buf[28..32].try_into().unwrap());
        let orientation = f32::from_le_bytes(buf[32..36].try_into().unwrap());
        let distance = f32::from_le_bytes(buf[36..40].try_into().unwrap());
        let is_resampled = u32::from_le_bytes(buf[40..44].try_into().unwrap()) != 0;

        Ok(Self {
            x,
            y,
            pressure,
            size,
            touch_major,
            touch_minor,
            tool_major,
            tool_minor,
            orientation,
            distance,
            is_resampled,
        })
    }
}

impl Parcelable for PointerCoords {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_f32(self.x).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.y).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.pressure).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.touch_major).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.touch_minor).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.tool_major).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.tool_minor).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.orientation).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.distance).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_bool(self.is_resampled).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.x = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.y = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.pressure = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.size = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.touch_major = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.touch_minor = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.tool_major = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.tool_minor = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.orientation = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.distance = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.is_resampled = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerProperties {
    pub id: i32,
    pub tool_type: i32,
}

impl Default for PointerProperties {
    fn default() -> Self {
        Self {
            id: 0,
            tool_type: 1, // TOOL_TYPE_FINGER
        }
    }
}

impl PointerProperties {
    pub fn new(id: i32, tool_type: i32) -> Self {
        Self { id, tool_type }
    }

    pub fn encode_bytes(&self, buf: &mut [u8]) {
        if buf.len() < 8 {
            return;
        }
        buf[0..4].copy_from_slice(&self.id.to_le_bytes());
        buf[4..8].copy_from_slice(&self.tool_type.to_le_bytes());
    }

    pub fn decode_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 8 {
            return Err(InputChannelError::InvalidMessageSize {
                expected: 8,
                actual: buf.len(),
            });
        }
        let id = i32::from_le_bytes(buf[0..4].try_into().unwrap());
        let tool_type = i32::from_le_bytes(buf[4..8].try_into().unwrap());
        Ok(Self { id, tool_type })
    }
}

// -----------------------------------------------------------------------------
// Key Event Data (Type = 1)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyEventData {
    pub seq: u32,
    pub event_time: i64,
    pub device_id: i32,
    pub source: u32,
    pub display_id: i32,
    pub action: i32,
    pub flags: i32,
    pub key_code: i32,
    pub scan_code: i32,
    pub meta_state: i32,
    pub repeat_count: i32,
    pub down_time: i64,
}

impl Default for KeyEventData {
    fn default() -> Self {
        Self {
            seq: 0,
            event_time: 0,
            device_id: 0,
            source: 0x00000101, // SOURCE_KEYBOARD
            display_id: 0,
            action: 0, // ACTION_DOWN
            flags: 0,
            key_code: 0,
            scan_code: 0,
            meta_state: 0,
            repeat_count: 0,
            down_time: 0,
        }
    }
}

impl KeyEventData {
    pub fn encode_bytes(&self, buf: &mut [u8]) {
        if buf.len() < 56 {
            return;
        }
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.event_time.to_le_bytes());
        buf[12..16].copy_from_slice(&self.device_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.source.to_le_bytes());
        buf[20..24].copy_from_slice(&self.display_id.to_le_bytes());
        buf[24..28].copy_from_slice(&self.action.to_le_bytes());
        buf[28..32].copy_from_slice(&self.flags.to_le_bytes());
        buf[32..36].copy_from_slice(&self.key_code.to_le_bytes());
        buf[36..40].copy_from_slice(&self.scan_code.to_le_bytes());
        buf[40..44].copy_from_slice(&self.meta_state.to_le_bytes());
        buf[44..48].copy_from_slice(&self.repeat_count.to_le_bytes());
        buf[48..56].copy_from_slice(&self.down_time.to_le_bytes());
    }

    pub fn decode_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 56 {
            return Err(InputChannelError::InvalidMessageSize {
                expected: 56,
                actual: buf.len(),
            });
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let event_time = i64::from_le_bytes(buf[4..12].try_into().unwrap());
        let device_id = i32::from_le_bytes(buf[12..16].try_into().unwrap());
        let source = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        let display_id = i32::from_le_bytes(buf[20..24].try_into().unwrap());
        let action = i32::from_le_bytes(buf[24..28].try_into().unwrap());
        let flags = i32::from_le_bytes(buf[28..32].try_into().unwrap());
        let key_code = i32::from_le_bytes(buf[32..36].try_into().unwrap());
        let scan_code = i32::from_le_bytes(buf[36..40].try_into().unwrap());
        let meta_state = i32::from_le_bytes(buf[40..44].try_into().unwrap());
        let repeat_count = i32::from_le_bytes(buf[44..48].try_into().unwrap());
        let down_time = i64::from_le_bytes(buf[48..56].try_into().unwrap());

        Ok(Self {
            seq,
            event_time,
            device_id,
            source,
            display_id,
            action,
            flags,
            key_code,
            scan_code,
            meta_state,
            repeat_count,
            down_time,
        })
    }
}

// -----------------------------------------------------------------------------
// Motion Event Data (Type = 2)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionEventData {
    pub seq: u32,
    pub event_time: i64,
    pub device_id: i32,
    pub source: u32,
    pub display_id: i32,
    pub action: i32,
    pub action_button: i32,
    pub flags: i32,
    pub edge_flags: i32,
    pub meta_state: i32,
    pub button_state: i32,
    pub classification: u8,
    pub x_precision: f32,
    pub y_precision: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub down_time: i64,
    pub pointer_count: u32,
    pub pointer_properties: [PointerProperties; MAX_POINTERS],
    pub pointer_coords: [PointerCoords; MAX_POINTERS],
}

impl Default for MotionEventData {
    fn default() -> Self {
        Self {
            seq: 0,
            event_time: 0,
            device_id: 0,
            source: 0x00001002, // SOURCE_TOUCHSCREEN
            display_id: 0,
            action: 0, // ACTION_DOWN
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
            down_time: 0,
            pointer_count: 1,
            pointer_properties: [PointerProperties::default(); MAX_POINTERS],
            pointer_coords: [PointerCoords::default(); MAX_POINTERS],
        }
    }
}

impl MotionEventData {
    pub fn encode_bytes(&self, buf: &mut [u8]) {
        // Base fields: 72 bytes
        // Pointer properties: 16 * 8 = 128 bytes
        // Pointer coords: 16 * 44 = 704 bytes -> clamped to available buf
        if buf.len() < 72 {
            return;
        }
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4..12].copy_from_slice(&self.event_time.to_le_bytes());
        buf[12..16].copy_from_slice(&self.device_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.source.to_le_bytes());
        buf[20..24].copy_from_slice(&self.display_id.to_le_bytes());
        buf[24..28].copy_from_slice(&self.action.to_le_bytes());
        buf[28..32].copy_from_slice(&self.action_button.to_le_bytes());
        buf[32..36].copy_from_slice(&self.flags.to_le_bytes());
        buf[36..40].copy_from_slice(&self.edge_flags.to_le_bytes());
        buf[40..44].copy_from_slice(&self.meta_state.to_le_bytes());
        buf[44..48].copy_from_slice(&self.button_state.to_le_bytes());
        buf[48] = self.classification;
        buf[49..52].fill(0); // padding
        buf[52..56].copy_from_slice(&self.x_precision.to_le_bytes());
        buf[56..60].copy_from_slice(&self.y_precision.to_le_bytes());
        buf[60..64].copy_from_slice(&self.x_offset.to_le_bytes());
        buf[64..68].copy_from_slice(&self.y_offset.to_le_bytes());
        buf[68..76].copy_from_slice(&self.down_time.to_le_bytes());
        buf[76..80].copy_from_slice(&self.pointer_count.to_le_bytes());

        let mut offset = 80;
        let count = (self.pointer_count as usize).min(MAX_POINTERS);

        // Encode active pointer properties
        for i in 0..count {
            if offset + 8 <= buf.len() {
                self.pointer_properties[i].encode_bytes(&mut buf[offset..offset + 8]);
                offset += 8;
            }
        }

        // Encode active pointer coords
        for i in 0..count {
            if offset + 44 <= buf.len() {
                self.pointer_coords[i].encode_bytes(&mut buf[offset..offset + 44]);
                offset += 44;
            }
        }
    }

    pub fn decode_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 80 {
            return Err(InputChannelError::InvalidMessageSize {
                expected: 80,
                actual: buf.len(),
            });
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let event_time = i64::from_le_bytes(buf[4..12].try_into().unwrap());
        let device_id = i32::from_le_bytes(buf[12..16].try_into().unwrap());
        let source = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        let display_id = i32::from_le_bytes(buf[20..24].try_into().unwrap());
        let action = i32::from_le_bytes(buf[24..28].try_into().unwrap());
        let action_button = i32::from_le_bytes(buf[28..32].try_into().unwrap());
        let flags = i32::from_le_bytes(buf[32..36].try_into().unwrap());
        let edge_flags = i32::from_le_bytes(buf[36..40].try_into().unwrap());
        let meta_state = i32::from_le_bytes(buf[40..44].try_into().unwrap());
        let button_state = i32::from_le_bytes(buf[44..48].try_into().unwrap());
        let classification = buf[48];
        let x_precision = f32::from_le_bytes(buf[52..56].try_into().unwrap());
        let y_precision = f32::from_le_bytes(buf[56..60].try_into().unwrap());
        let x_offset = f32::from_le_bytes(buf[60..64].try_into().unwrap());
        let y_offset = f32::from_le_bytes(buf[64..68].try_into().unwrap());
        let down_time = i64::from_le_bytes(buf[68..76].try_into().unwrap());
        let pointer_count = u32::from_le_bytes(buf[76..80].try_into().unwrap());

        let mut pointer_properties = [PointerProperties::default(); MAX_POINTERS];
        let mut pointer_coords = [PointerCoords::default(); MAX_POINTERS];

        let mut offset = 80;
        let count = (pointer_count as usize).min(MAX_POINTERS);

        for prop in pointer_properties.iter_mut().take(count) {
            if offset + 8 <= buf.len() {
                *prop = PointerProperties::decode_bytes(&buf[offset..offset + 8])?;
                offset += 8;
            }
        }

        for coord in pointer_coords.iter_mut().take(count) {
            if offset + 44 <= buf.len() {
                *coord = PointerCoords::decode_bytes(&buf[offset..offset + 44])?;
                offset += 44;
            }
        }

        Ok(Self {
            seq,
            event_time,
            device_id,
            source,
            display_id,
            action,
            action_button,
            flags,
            edge_flags,
            meta_state,
            button_state,
            classification,
            x_precision,
            y_precision,
            x_offset,
            y_offset,
            down_time,
            pointer_count,
            pointer_properties,
            pointer_coords,
        })
    }
}

// -----------------------------------------------------------------------------
// Finished Signal Data (Type = 3)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FinishedData {
    pub seq: u32,
    pub handled: bool,
    pub consume_time: i64,
}

impl Default for FinishedData {
    fn default() -> Self {
        Self {
            seq: 0,
            handled: true,
            consume_time: 0,
        }
    }
}

impl FinishedData {
    pub fn new(seq: u32, handled: bool) -> Self {
        Self {
            seq,
            handled,
            consume_time: 0,
        }
    }

    pub fn encode_bytes(&self, buf: &mut [u8]) {
        if buf.len() < 16 {
            return;
        }
        buf[0..4].copy_from_slice(&self.seq.to_le_bytes());
        buf[4] = if self.handled { 1 } else { 0 };
        buf[5..8].fill(0); // padding
        buf[8..16].copy_from_slice(&self.consume_time.to_le_bytes());
    }

    pub fn decode_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 16 {
            return Err(InputChannelError::InvalidMessageSize {
                expected: 16,
                actual: buf.len(),
            });
        }
        let seq = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let handled = buf[4] != 0;
        let consume_time = i64::from_le_bytes(buf[8..16].try_into().unwrap());

        Ok(Self {
            seq,
            handled,
            consume_time,
        })
    }
}

// -----------------------------------------------------------------------------
// InputMessage Enum & Binary Codec
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum InputMessage {
    Key(KeyEventData),
    Motion(MotionEventData),
    Finished(FinishedData),
}

impl InputMessage {
    pub fn message_type(&self) -> u32 {
        match self {
            InputMessage::Key(_) => INPUT_MESSAGE_TYPE_KEY,
            InputMessage::Motion(_) => INPUT_MESSAGE_TYPE_MOTION,
            InputMessage::Finished(_) => INPUT_MESSAGE_TYPE_FINISHED,
        }
    }

    pub fn seq(&self) -> u32 {
        match self {
            InputMessage::Key(k) => k.seq,
            InputMessage::Motion(m) => m.seq,
            InputMessage::Finished(f) => f.seq,
        }
    }

    /// Encode into fixed 512-byte wire buffer for socket transmission.
    pub fn encode(&self, dst: &mut [u8; INPUT_MESSAGE_WIRE_SIZE]) {
        dst.fill(0);
        let msg_type = self.message_type();
        let seq = self.seq();

        dst[0..4].copy_from_slice(&msg_type.to_le_bytes());
        dst[4..8].copy_from_slice(&seq.to_le_bytes());

        match self {
            InputMessage::Key(k) => {
                k.encode_bytes(&mut dst[8..]);
            }
            InputMessage::Motion(m) => {
                m.encode_bytes(&mut dst[8..]);
            }
            InputMessage::Finished(f) => {
                f.encode_bytes(&mut dst[8..]);
            }
        }
    }

    /// Serialize message to vector of bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = [0u8; INPUT_MESSAGE_WIRE_SIZE];
        self.encode(&mut buf);
        buf.to_vec()
    }

    /// Decode from binary wire buffer.
    pub fn decode(src: &[u8]) -> Result<Self> {
        if src.len() < INPUT_MESSAGE_HEADER_SIZE {
            return Err(InputChannelError::InvalidMessageSize {
                expected: INPUT_MESSAGE_HEADER_SIZE,
                actual: src.len(),
            });
        }

        let msg_type = u32::from_le_bytes(src[0..4].try_into().unwrap());
        let _seq = u32::from_le_bytes(src[4..8].try_into().unwrap());

        let payload = &src[8..];
        match msg_type {
            INPUT_MESSAGE_TYPE_KEY => {
                let key_data = KeyEventData::decode_bytes(payload)?;
                Ok(InputMessage::Key(key_data))
            }
            INPUT_MESSAGE_TYPE_MOTION => {
                let motion_data = MotionEventData::decode_bytes(payload)?;
                Ok(InputMessage::Motion(motion_data))
            }
            INPUT_MESSAGE_TYPE_FINISHED => {
                let finished_data = FinishedData::decode_bytes(payload)?;
                Ok(InputMessage::Finished(finished_data))
            }
            other => Err(InputChannelError::UnknownMessageType(other)),
        }
    }
}

impl Parcelable for InputMessage {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        let bytes = self.serialize();
        parcel.write_byte_slice(Some(&bytes)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let bytes = parcel.read_byte_vec(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        *self = InputMessage::decode(&bytes).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_roundtrip() {
        let mut key = KeyEventData::default();
        key.seq = 42;
        key.key_code = 29; // KEYCODE_A
        key.action = 0; // ACTION_DOWN
        key.event_time = 123456789;
        key.down_time = 123456780;

        let msg = InputMessage::Key(key.clone());
        let encoded = msg.serialize();
        let decoded = InputMessage::decode(&encoded).expect("Decode key message failed");

        if let InputMessage::Key(decoded_key) = decoded {
            assert_eq!(decoded_key, key);
        } else {
            panic!("Expected InputMessage::Key");
        }
    }

    #[test]
    fn test_motion_event_roundtrip() {
        let mut motion = MotionEventData::default();
        motion.seq = 100;
        motion.action = 2; // ACTION_MOVE
        motion.pointer_count = 2;
        motion.pointer_properties[0] = PointerProperties::new(0, 1);
        motion.pointer_properties[1] = PointerProperties::new(1, 1);
        motion.pointer_coords[0] = PointerCoords::new(150.0, 300.0, 0.8, 1.2);
        motion.pointer_coords[1] = PointerCoords::new(250.0, 400.0, 0.9, 1.5);
        motion.event_time = 987654321;

        let msg = InputMessage::Motion(motion.clone());
        let encoded = msg.serialize();
        let decoded = InputMessage::decode(&encoded).expect("Decode motion message failed");

        if let InputMessage::Motion(decoded_motion) = decoded {
            assert_eq!(decoded_motion.seq, motion.seq);
            assert_eq!(decoded_motion.pointer_count, motion.pointer_count);
            assert_eq!(decoded_motion.pointer_coords[0].x, 150.0);
            assert_eq!(decoded_motion.pointer_coords[1].y, 400.0);
        } else {
            panic!("Expected InputMessage::Motion");
        }
    }

    #[test]
    fn test_finished_signal_roundtrip() {
        let finished = FinishedData::new(55, true);
        let msg = InputMessage::Finished(finished);
        let encoded = msg.serialize();
        let decoded = InputMessage::decode(&encoded).expect("Decode finished signal failed");

        if let InputMessage::Finished(decoded_fin) = decoded {
            assert_eq!(decoded_fin.seq, 55);
            assert!(decoded_fin.handled);
        } else {
            panic!("Expected InputMessage::Finished");
        }
    }
}
