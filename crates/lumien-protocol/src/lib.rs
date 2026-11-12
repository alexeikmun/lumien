//! Wire framing and Protobuf message codec for TRYX Panorama SE display controllers.
//!
//! Reconstructed from TRYX KANALI UDB.exe (Windows 11 x64).

pub const FRAME_MAGIC: [u8; 4] = *b"TRYX";
pub const FRAME_HEADER_LEN: usize = 8;
pub const CANVAS_WIDTH: u32 = 2240;
pub const CANVAS_HEIGHT: u32 = 1080;

#[derive(Debug, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidMagic([u8; 4]),
    BufferTooShort { expected: usize, actual: usize },
    InvalidVarint,
    UnexpectedTag { expected: u32, actual: u32 },
    InvalidResponse,
    InvalidGeometry(&'static str),
    InvalidField(&'static str),
}

/// Encapsulates a Protobuf payload in the 8-byte TRYX framing container.
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&FRAME_MAGIC);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Decodes an 8-byte container frame, returning `(payload, remaining_bytes)`.
pub fn decode_frame(buf: &[u8]) -> Result<(&[u8], &[u8]), ProtocolError> {
    if buf.len() < FRAME_HEADER_LEN {
        return Err(ProtocolError::BufferTooShort {
            expected: FRAME_HEADER_LEN,
            actual: buf.len(),
        });
    }

    let magic: [u8; 4] = buf[0..4].try_into().unwrap();
    if magic != FRAME_MAGIC {
        return Err(ProtocolError::InvalidMagic(magic));
    }

    let payload_len = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    let total_len = FRAME_HEADER_LEN + payload_len;

    if buf.len() < total_len {
        return Err(ProtocolError::BufferTooShort {
            expected: total_len,
            actual: buf.len(),
        });
    }

    Ok((&buf[FRAME_HEADER_LEN..total_len], &buf[total_len..]))
}

// ponytail: handwritten minimal protobuf varint / tag codec avoids heavy prost/protobuf dependencies.
// Upgrade path: pull prost if full complex RPC schemas are needed.
fn encode_varint(mut val: u64, buf: &mut Vec<u8>) {
    while val >= 0x80 {
        buf.push(((val & 0x7F) as u8) | 0x80);
        val >>= 7;
    }
    buf.push((val & 0x7F) as u8);
}

fn decode_varint(buf: &[u8], mut offset: usize) -> Result<(u64, usize), ProtocolError> {
    let mut result: u64 = 0;
    let mut shift = 0;
    while offset < buf.len() {
        let b = buf[offset];
        offset += 1;
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Ok((result, offset));
        }
        shift += 7;
        if shift >= 64 {
            return Err(ProtocolError::InvalidVarint);
        }
    }
    Err(ProtocolError::BufferTooShort {
        expected: offset + 1,
        actual: buf.len(),
    })
}

fn encode_tag(field_num: u32, wire_type: u8, buf: &mut Vec<u8>) {
    encode_varint(((field_num as u64) << 3) | (wire_type as u64), buf);
}

fn encode_length_delimited(field_num: u32, data: &[u8], buf: &mut Vec<u8>) {
    encode_tag(field_num, 2, buf);
    encode_varint(data.len() as u64, buf);
    buf.extend_from_slice(data);
}

fn encode_zigzag32(n: i32) -> u64 {
    ((n << 1) ^ (n >> 31)) as u32 as u64
}

fn encode_varint_field(field_num: u32, val: u64, buf: &mut Vec<u8>) {
    encode_tag(field_num, 0, buf);
    encode_varint(val, buf);
}

fn encode_sint32_field(field_num: u32, val: i32, buf: &mut Vec<u8>) {
    encode_tag(field_num, 0, buf);
    encode_varint(encode_zigzag32(val), buf);
}

fn encode_string_field(field_num: u32, s: &str, buf: &mut Vec<u8>) {
    encode_length_delimited(field_num, s.as_bytes(), buf);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left = 0,
    Center = 1,
    Right = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelBackground {
    None = 0,
    Solid = 1,
    GradientHorizontal = 2,
    GradientVertical = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub label_id: u32,
    pub line: u32,
    pub order: u32,
    pub gap_left: i32,
    pub background: LabelBackground,
    pub background_color: u32,
    pub gradient_color: u32,
    pub text_font: String,
    pub text_size: u32,
    pub text_color: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelGroup {
    pub group_id: u32,
    pub group_x: u32,
    pub group_y: u32,
    pub group_width: u32,
    pub group_height: u32,
    pub text_align: TextAlign,
    pub line_gap: i32,
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelTextUpdate {
    pub label_id: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupLabelUpdate {
    pub group_id: u32,
    pub label_texts: Vec<LabelTextUpdate>,
}

pub fn validate_group_geometry(g: &LabelGroup) -> Result<(), ProtocolError> {
    if g.group_id == 0 {
        return Err(ProtocolError::InvalidField("group_id must be non-zero"));
    }
    if g.group_width == 0 || g.group_height == 0 {
        return Err(ProtocolError::InvalidGeometry("group dimensions must be non-zero"));
    }
    if g.group_x + g.group_width > CANVAS_WIDTH {
        return Err(ProtocolError::InvalidGeometry("group_x + group_width exceeds 2240"));
    }
    if g.group_y + g.group_height > CANVAS_HEIGHT {
        return Err(ProtocolError::InvalidGeometry("group_y + group_height exceeds 1080"));
    }
    if g.labels.is_empty() {
        return Err(ProtocolError::InvalidField("group must contain at least one label"));
    }
    for l in &g.labels {
        if l.label_id == 0 {
            return Err(ProtocolError::InvalidField("label_id must be non-zero"));
        }
        if l.text_font.is_empty() || l.text_font.len() > 64 {
            return Err(ProtocolError::InvalidField("text_font must be 1..64 chars"));
        }
        if l.text.len() > 500 {
            return Err(ProtocolError::InvalidField("text must be <= 500 chars"));
        }
        if l.text_size == 0 || l.text_size > 300 {
            return Err(ProtocolError::InvalidField("text_size must be 1..300"));
        }
    }
    Ok(())
}

fn encode_label(l: &Label, buf: &mut Vec<u8>) {
    let mut inner = Vec::with_capacity(64);
    encode_varint_field(1, l.label_id as u64, &mut inner);
    encode_varint_field(2, l.line as u64, &mut inner);
    encode_varint_field(3, l.order as u64, &mut inner);
    encode_sint32_field(4, l.gap_left, &mut inner);
    if l.background != LabelBackground::None {
        encode_varint_field(5, l.background as u64, &mut inner);
        encode_varint_field(6, l.background_color as u64, &mut inner);
        encode_varint_field(7, l.gradient_color as u64, &mut inner);
    }
    encode_string_field(8, &l.text_font, &mut inner);
    encode_varint_field(9, l.text_size as u64, &mut inner);
    encode_varint_field(10, l.text_color as u64, &mut inner);
    encode_string_field(11, &l.text, &mut inner);

    encode_length_delimited(8, &inner, buf);
}

fn encode_group(g: &LabelGroup, buf: &mut Vec<u8>) {
    let mut inner = Vec::with_capacity(128);
    encode_varint_field(1, g.group_id as u64, &mut inner);
    encode_varint_field(2, g.group_x as u64, &mut inner);
    encode_varint_field(3, g.group_y as u64, &mut inner);
    encode_varint_field(4, g.group_width as u64, &mut inner);
    encode_varint_field(5, g.group_height as u64, &mut inner);
    encode_varint_field(6, g.text_align as u64, &mut inner);
    encode_sint32_field(7, g.line_gap, &mut inner);
    for l in &g.labels {
        encode_label(l, &mut inner);
    }
    encode_length_delimited(1, &inner, buf);
}

/// Encodes a complete RunConfigPb frame (ReqPackagePb tag 201) with geometry validation.
pub fn encode_run_config_frame(groups: &[LabelGroup]) -> Result<Vec<u8>, ProtocolError> {
    let mut run_config = Vec::with_capacity(256);
    for g in groups {
        validate_group_geometry(g)?;
        encode_group(g, &mut run_config);
    }

    let mut req_pkg = Vec::with_capacity(run_config.len() + 16);
    let header_bytes = [0x08, 0x01]; // HeaderPb { version: 1 }
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    // Field 201 is run_config (.Tryx.Config.RunConfigPb)
    encode_length_delimited(201, &run_config, &mut req_pkg);

    Ok(encode_frame(&req_pkg))
}

/// Encodes an empty RunConfigPb frame (ReqPackagePb tag 201) to clear all widgets on the display.
pub fn encode_clear_run_config_frame() -> Vec<u8> {
    let mut req_pkg = Vec::with_capacity(16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(201, &[], &mut req_pkg); // empty RunConfigPb
    encode_frame(&req_pkg)
}

/// Encodes a BatchGroupLabelUpdatePb frame (ReqPackagePb tag 300) for runtime 1 Hz telemetry text updates.
pub fn encode_batch_label_update_frame(updates: &[GroupLabelUpdate]) -> Result<Vec<u8>, ProtocolError> {
    let mut batch = Vec::with_capacity(64);
    for group_up in updates {
        let mut g_inner = Vec::with_capacity(48);
        encode_varint_field(1, group_up.group_id as u64, &mut g_inner);
        for l_up in &group_up.label_texts {
            let mut l_inner = Vec::with_capacity(24);
            encode_varint_field(1, l_up.label_id as u64, &mut l_inner);
            encode_string_field(2, &l_up.text, &mut l_inner);
            encode_length_delimited(2, &l_inner, &mut g_inner);
        }
        encode_length_delimited(1, &g_inner, &mut batch);
    }

    let mut req_pkg = Vec::with_capacity(batch.len() + 16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    // Field 300 is batch_group_label_update (.Tryx.LVGui.BatchGroupLabelUpdatePb)
    encode_length_delimited(300, &batch, &mut req_pkg);

    Ok(encode_frame(&req_pkg))
}

/// Encodes a complete 17-byte get_device_auth query frame.
///
/// Wraps:
/// - HeaderPb { version: 1 } (tag 1)
/// - GetDeviceAuthPb { key: 1 } (tag 101)
pub fn encode_get_auth_frame() -> Vec<u8> {
    let mut auth_msg = Vec::with_capacity(2);
    encode_tag(1, 0, &mut auth_msg);
    auth_msg.push(1); // key = 1

    let mut req_pkg = Vec::with_capacity(16);
    let header_bytes = [0x08, 0x01]; // field 1, varint 1
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(101, &auth_msg, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Encodes a complete 21-byte brightness command frame.
///
/// Wraps:
/// - HeaderPb { version: 1 } (tag 1)
/// - UserConfigPb { display_config: DisplayConfigPb { backlight_enable: true, backlight_brightness: N } } (tag 200)
pub fn encode_brightness_frame(brightness: u8) -> Vec<u8> {
    // DisplayConfigPb:
    // field 1 (backlight_enable) = true
    // field 2 (backlight_brightness) = brightness (0..100)
    let mut display_cfg = Vec::with_capacity(4);
    encode_tag(1, 0, &mut display_cfg);
    display_cfg.push(1); // true
    encode_tag(2, 0, &mut display_cfg);
    encode_varint(brightness as u64, &mut display_cfg);

    // UserConfigPb:
    // field 5 (display_config) = display_cfg
    let mut user_cfg = Vec::with_capacity(6);
    encode_length_delimited(5, &display_cfg, &mut user_cfg);

    // ReqPackagePb:
    // field 1 (header) = HeaderPb { version: 1 }
    // field 200 (user_config) = user_cfg
    let mut req_pkg = Vec::with_capacity(16);
    let header_bytes = [0x08, 0x01]; // field 1, varint 1
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(200, &user_cfg, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Precomputed 256-entry table for Castagnoli CRC32C (polynomial 0x82F63B78).
const CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0x82F63B78;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Computes IEEE standard Castagnoli CRC32C checksum (Refin=true, Refout=true, Poly=0x82F63B78).
///
/// Verified identical to TRYX UDB.exe native implementation at address 0x4d970.
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = CRC32C_TABLE[((crc ^ (b as u32)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

/// Encodes a complete FileTransmitBeginPb frame (ReqPackagePb tag 400).
pub fn encode_file_transmit_begin_frame(file_name: &str, file_size: u32) -> Vec<u8> {
    let mut inner = Vec::with_capacity(file_name.len() + 16);
    encode_string_field(1, file_name, &mut inner);
    encode_varint_field(2, file_size as u64, &mut inner);

    let mut req_pkg = Vec::with_capacity(inner.len() + 16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(400, &inner, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Encodes a complete FileTransmitDataPb frame (ReqPackagePb tag 401).
pub fn encode_file_transmit_data_frame(chunk: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(chunk.len() + 8);
    encode_length_delimited(1, chunk, &mut inner);

    let mut req_pkg = Vec::with_capacity(inner.len() + 16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(401, &inner, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Encodes a complete FileTransmitEndPb frame (ReqPackagePb tag 402) with CRC32C.
pub fn encode_file_transmit_end_frame(file_type: &str, crc: u32) -> Vec<u8> {
    let mut inner = Vec::with_capacity(file_type.len() + 16);
    encode_string_field(1, file_type, &mut inner);
    encode_varint_field(2, crc as u64, &mut inner);

    let mut req_pkg = Vec::with_capacity(inner.len() + 16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(402, &inner, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Encodes a complete WorkConfigPb frame (ReqPackagePb tag 200 -> tag 3)
/// to switch active background media playback on the device.
pub fn encode_work_config_frame(media_file: &str) -> Vec<u8> {
    // WorkConfigPb:
    // field 1 (media_mode) = 0 (MediaMode_Single)
    // field 2 (loop_mode) = 0 (LoopMode_Single)
    // field 3 (single_mode_media_file) = media_file
    let mut work_cfg = Vec::with_capacity(media_file.len() + 16);
    encode_varint_field(1, 0, &mut work_cfg);
    encode_varint_field(2, 0, &mut work_cfg);
    encode_string_field(3, media_file, &mut work_cfg);

    // UserConfigPb:
    // field 3 (work_config) = work_cfg
    let mut user_cfg = Vec::with_capacity(work_cfg.len() + 8);
    encode_length_delimited(3, &work_cfg, &mut user_cfg);

    // ReqPackagePb:
    // field 1 (header) = HeaderPb { version: 1 }
    // field 200 (user_config) = user_cfg
    let mut req_pkg = Vec::with_capacity(user_cfg.len() + 16);
    let header_bytes = [0x08, 0x01];
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(200, &user_cfg, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Encodes a complete 19-byte get_device_info query frame.
///
/// Wraps:
/// - HeaderPb { version: 1 } (tag 1)
/// - DummyMsgPb { dummy: "NA" } (tag 100)
pub fn encode_get_device_info_frame() -> Vec<u8> {
    let mut dummy_msg = Vec::with_capacity(4);
    encode_length_delimited(1, b"NA", &mut dummy_msg);

    let mut req_pkg = Vec::with_capacity(16);
    let header_bytes = [0x08, 0x01]; // field 1, varint 1
    encode_length_delimited(1, &header_bytes, &mut req_pkg);
    encode_length_delimited(100, &dummy_msg, &mut req_pkg);

    encode_frame(&req_pkg)
}

/// Parsed device information from `DeviceInfoPb` (tag 500 in `RspPackagePb`).
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct DeviceInfo {
    pub os_name: String,
    pub os_version: String,
    pub firmware_version: String,
    pub product_name: String,
    pub app_version: String,
    pub serial_number: String,
    pub chip_id: String,
}

/// Parsed acknowledgment or response payload from device response (`RspPackagePb`).
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceResponse {
    Ack(DeviceAck),
    Info(DeviceInfo),
    Error { code: String, why: String },
}

/// Parses any incoming response frame from Bulk IN endpoint 0x81.
pub fn parse_device_response(frame_bytes: &[u8]) -> Result<DeviceResponse, ProtocolError> {
    let (payload, _) = decode_frame(frame_bytes)?;
    let mut offset = 0;
    let mut version = 0;
    let mut ok = false;
    let mut device_info = None;
    let mut err_code = String::new();
    let mut err_why = String::new();

    while offset < payload.len() {
        let (tag_wire, new_offset) = decode_varint(payload, offset)?;
        offset = new_offset;
        let field_num = (tag_wire >> 3) as u32;
        let wire_type = (tag_wire & 0x7) as u8;

        match (field_num, wire_type) {
            (1, 2) => {
                // HeaderPb length-delimited
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                if end > payload.len() {
                    return Err(ProtocolError::BufferTooShort { expected: end, actual: payload.len() });
                }
                let mut h_off = offset;
                while h_off < end {
                    let (h_tag, next_h) = decode_varint(payload, h_off)?;
                    h_off = next_h;
                    if (h_tag >> 3) == 1 && (h_tag & 7) == 0 {
                        let (v, next_v) = decode_varint(payload, h_off)?;
                        version = v as u32;
                        h_off = next_v;
                    }
                }
                offset = end;
            }
            (2, 2) => {
                // ErrorPb length-delimited
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                if end > payload.len() {
                    return Err(ProtocolError::BufferTooShort { expected: end, actual: payload.len() });
                }
                let mut e_off = offset;
                while e_off < end {
                    let (e_tag, next_e) = decode_varint(payload, e_off)?;
                    e_off = next_e;
                    let f = (e_tag >> 3) as u32;
                    let w = (e_tag & 7) as u8;
                    if w == 2 {
                        let (s_len, next_s) = decode_varint(payload, e_off)?;
                        e_off = next_s;
                        let s_end = e_off + s_len as usize;
                        let s = String::from_utf8_lossy(&payload[e_off..s_end]).to_string();
                        if f == 1 { err_code = s; } else if f == 2 { err_why = s; }
                        e_off = s_end;
                    }
                }
                offset = end;
            }
            (500, 2) => {
                // DeviceInfoPb length-delimited
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                if end > payload.len() {
                    return Err(ProtocolError::BufferTooShort { expected: end, actual: payload.len() });
                }
                let mut info = DeviceInfo::default();
                let mut i_off = offset;
                while i_off < end {
                    let (i_tag, next_i) = decode_varint(payload, i_off)?;
                    i_off = next_i;
                    let f = (i_tag >> 3) as u32;
                    let w = (i_tag & 7) as u8;
                    if w == 2 {
                        let (s_len, next_s) = decode_varint(payload, i_off)?;
                        i_off = next_s;
                        let s_end = i_off + s_len as usize;
                        let val = String::from_utf8_lossy(&payload[i_off..s_end]).to_string();
                        match f {
                            1 => info.os_name = val,
                            2 => info.os_version = val,
                            3 => info.firmware_version = val,
                            4 => info.product_name = val,
                            5 => info.app_version = val,
                            8 => info.serial_number = val,
                            10 => info.chip_id = val,
                            _ => {}
                        }
                        i_off = s_end;
                    } else if w == 0 {
                        let (_, next_v) = decode_varint(payload, i_off)?;
                        i_off = next_v;
                    }
                }
                device_info = Some(info);
                offset = end;
            }
            (501, 2) => {
                // DeviceAuthPb length-delimited: field 1 = string "ok"
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                if payload[offset..end] == [0x0a, 0x02, b'o', b'k'] {
                    ok = true;
                }
                offset = end;
            }
            (600, 2) => {
                // DummyMsgPb length-delimited: field 1 = string "ok"
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                if payload[offset..end] == [0x0a, 0x02, b'o', b'k'] {
                    ok = true;
                }
                offset = end;
            }
            (800, 2) | (801, 2) | (802, 2) => {
                // FileTransmitStatusPb / FileTransmitDataStatusPb / FileTransmitEndStatusPb:
                // field 1 (file_trans_status) varint: 0 = OK, 1 = SpaceNotEnough, 2 = FileError, 3 = CRCFail
                let (len, new_offset) = decode_varint(payload, offset)?;
                offset = new_offset;
                let end = offset + len as usize;
                let mut status = 0u32;
                let mut s_off = offset;
                while s_off < end {
                    let (s_tag, next_s) = decode_varint(payload, s_off)?;
                    s_off = next_s;
                    if (s_tag >> 3) == 1 && (s_tag & 7) == 0 {
                        let (s_val, next_v) = decode_varint(payload, s_off)?;
                        status = s_val as u32;
                        s_off = next_v;
                    }
                }
                if status == 0 {
                    ok = true;
                } else {
                    ok = false;
                    err_code = match status {
                        1 => "SpaceNotEnough".to_string(),
                        2 => "FileError".to_string(),
                        3 => "CRCFail".to_string(),
                        n => format!("FileTransStatus_{}", n),
                    };
                }
                offset = end;
            }
            _ => {
                // Skip unknown length-delimited or varint
                if wire_type == 2 {
                    let (len, new_offset) = decode_varint(payload, offset)?;
                    offset = new_offset + len as usize;
                } else if wire_type == 0 {
                    let (_, new_offset) = decode_varint(payload, offset)?;
                    offset = new_offset;
                } else {
                    return Err(ProtocolError::InvalidResponse);
                }
            }
        }
    }

    if !err_code.is_empty() || !err_why.is_empty() {
        return Ok(DeviceResponse::Error { code: err_code, why: err_why });
    }

    if let Some(info) = device_info {
        return Ok(DeviceResponse::Info(info));
    }

    Ok(DeviceResponse::Ack(DeviceAck { version, ok }))
}

/// Parsed acknowledgment from device response (`RspPackagePb`).
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceAck {
    pub version: u32,
    pub ok: bool,
}

/// Parses a 20-byte response frame from Bulk IN endpoint 0x81.
pub fn parse_response_frame(frame_bytes: &[u8]) -> Result<DeviceAck, ProtocolError> {
    match parse_device_response(frame_bytes)? {
        DeviceResponse::Ack(ack) => Ok(ack),
        _ => Err(ProtocolError::InvalidResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verified capture fixtures from live KANALI trace and decompiled UDB.exe:
    const FIXTURE_REQ_10: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0xc2, 0x0c, 0x06, 0x2a, 0x04, 0x08, 0x01, 0x10, 0x0a,
    ];
    const FIXTURE_REQ_28: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0xc2, 0x0c, 0x06, 0x2a, 0x04, 0x08, 0x01, 0x10, 0x1c,
    ];
    const FIXTURE_REQ_42: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0xc2, 0x0c, 0x06, 0x2a, 0x04, 0x08, 0x01, 0x10, 0x2a,
    ];
    const FIXTURE_REQ_50: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0xc2, 0x0c, 0x06, 0x2a, 0x04, 0x08, 0x01, 0x10, 0x32,
    ];
    const FIXTURE_REQ_100: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0xc2, 0x0c, 0x06, 0x2a, 0x04, 0x08, 0x01, 0x10, 0x64,
    ];
    const FIXTURE_RSP_OK: &[u8] = &[
        0x54, 0x52, 0x59, 0x58, 0x0d, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0x12, 0x00, 0xc2, 0x25, 0x04, 0x0a, 0x02, 0x6f, 0x6b,
    ];

    #[test]
    fn test_encode_get_device_info_frame() {
        let frame = encode_get_device_info_frame();
        assert_eq!(
            frame,
            &[
                0x54, 0x52, 0x59, 0x58, // TRYX
                0x0b, 0x00, 0x00, 0x00, // length 11
                0x0a, 0x02, 0x08, 0x01, // header { version: 1 }
                0xa2, 0x06, 0x04, 0x0a, 0x02, 0x4e, 0x41, // get_device_info { dummy: "NA" }
            ]
        );
    }

    #[test]
    fn test_encode_get_auth_frame() {
        let frame = encode_get_auth_frame();
        assert_eq!(
            frame,
            &[
                0x54, 0x52, 0x59, 0x58, // TRYX
                0x09, 0x00, 0x00, 0x00, // length 9
                0x0a, 0x02, 0x08, 0x01, // header { version: 1 }
                0xaa, 0x06, 0x02, 0x08, 0x01, // get_device_auth { key: 1 }
            ]
        );
    }

    #[test]
    fn test_encode_brightness_matches_captured_fixtures() {
        assert_eq!(encode_brightness_frame(10), FIXTURE_REQ_10);
        assert_eq!(encode_brightness_frame(28), FIXTURE_REQ_28);
        assert_eq!(encode_brightness_frame(42), FIXTURE_REQ_42);
        assert_eq!(encode_brightness_frame(50), FIXTURE_REQ_50);
        assert_eq!(encode_brightness_frame(100), FIXTURE_REQ_100);
    }

    #[test]
    fn test_decode_frame_container() {
        let (payload, remaining) = decode_frame(FIXTURE_REQ_50).expect("valid frame");
        assert_eq!(payload.len(), 13);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_parse_response_frame() {
        let ack = parse_response_frame(FIXTURE_RSP_OK).expect("valid response");
        assert_eq!(ack, DeviceAck { version: 1, ok: true });
    }

    #[test]
    fn test_corrupt_magic_rejected() {
        let mut corrupt = FIXTURE_REQ_50.to_vec();
        corrupt[0] = b'X';
        assert_eq!(
            decode_frame(&corrupt),
            Err(ProtocolError::InvalidMagic([b'X', b'R', b'Y', b'X']))
        );
    }

    #[test]
    fn test_truncated_buffer_rejected() {
        assert!(matches!(
            decode_frame(&FIXTURE_REQ_50[..6]),
            Err(ProtocolError::BufferTooShort { expected: 8, actual: 6 })
        ));
    }

    #[test]
    fn test_encode_run_config_frame_valid() {
        let label1 = Label {
            label_id: 101,
            line: 1,
            order: 0,
            gap_left: 13,
            background: LabelBackground::None,
            background_color: 0,
            gradient_color: 0,
            text_font: "roboto-regular".to_string(),
            text_size: 30,
            text_color: 14474460,
            text: "CPU TEMP".to_string(),
        };
        let label2 = Label {
            label_id: 102,
            line: 0,
            order: 1,
            gap_left: 0,
            background: LabelBackground::None,
            background_color: 0,
            gradient_color: 0,
            text_font: "roboto-regular".to_string(),
            text_size: 160,
            text_color: 14474460,
            text: "43".to_string(),
        };
        let group = LabelGroup {
            group_id: 100,
            group_x: 60,
            group_y: 260,
            group_width: 2120,
            group_height: 160,
            text_align: TextAlign::Left,
            line_gap: -10,
            labels: vec![label1, label2],
        };

        let frame = encode_run_config_frame(&[group]).expect("valid encoding");
        // Verify container header
        assert_eq!(&frame[0..4], b"TRYX");
        let payload_len = u32::from_le_bytes(frame[4..8].try_into().unwrap());
        assert_eq!(frame.len(), 8 + payload_len as usize);

        // HeaderPb version 1 (tag 1: 0a 02 08 01)
        assert_eq!(&frame[8..12], &[0x0a, 0x02, 0x08, 0x01]);
        // Field 201 tag (ca 0c) for run_config
        assert_eq!(&frame[12..14], &[0xca, 0x0c]);
    }

    #[test]
    fn test_encode_run_config_geometry_out_of_bounds() {
        let label = Label {
            label_id: 1,
            line: 0,
            order: 0,
            gap_left: 0,
            background: LabelBackground::None,
            background_color: 0,
            gradient_color: 0,
            text_font: "roboto-regular".to_string(),
            text_size: 40,
            text_color: 0xFFFFFF,
            text: "Test".to_string(),
        };
        // group_x (2200) + group_width (100) = 2300 > 2240
        let group_x_overflow = LabelGroup {
            group_id: 1,
            group_x: 2200,
            group_y: 100,
            group_width: 100,
            group_height: 100,
            text_align: TextAlign::Left,
            line_gap: 0,
            labels: vec![label.clone()],
        };
        assert!(matches!(
            encode_run_config_frame(&[group_x_overflow]),
            Err(ProtocolError::InvalidGeometry(_))
        ));

        // group_y (1000) + group_height (100) = 1100 > 1080
        let group_y_overflow = LabelGroup {
            group_id: 1,
            group_x: 100,
            group_y: 1000,
            group_width: 100,
            group_height: 100,
            text_align: TextAlign::Left,
            line_gap: 0,
            labels: vec![label],
        };
        assert!(matches!(
            encode_run_config_frame(&[group_y_overflow]),
            Err(ProtocolError::InvalidGeometry(_))
        ));
    }

    #[test]
    fn test_encode_clear_run_config_frame() {
        let frame = encode_clear_run_config_frame();
        assert_eq!(&frame[0..4], b"TRYX");
        // header (4 bytes) + field 201 empty (3 bytes: ca 0c 00)
        assert_eq!(&frame[8..12], &[0x0a, 0x02, 0x08, 0x01]);
        assert_eq!(&frame[12..15], &[0xca, 0x0c, 0x00]);
    }

    #[test]
    fn test_encode_batch_label_update_frame() {
        let update = GroupLabelUpdate {
            group_id: 100,
            label_texts: vec![LabelTextUpdate {
                label_id: 102,
                text: "45".to_string(),
            }],
        };
        let frame = encode_batch_label_update_frame(&[update]).expect("valid batch update");
        assert_eq!(&frame[0..4], b"TRYX");
        // Tag 300 prefix (e2 12)
        assert_eq!(&frame[12..14], &[0xe2, 0x12]);
        // Exact 27-byte match with verified wire sequence
        assert_eq!(
            frame,
            &[
                0x54, 0x52, 0x59, 0x58, // TRYX
                0x13, 0x00, 0x00, 0x00, // payload length 19
                0x0a, 0x02, 0x08, 0x01, // header { version: 1 }
                0xe2, 0x12, 0x0c,       // batch_group_label_update (tag 300, len 12)
                0x0a, 0x0a,             // GroupLabelUpdate (tag 1, len 10)
                0x08, 0x64,             // group_id = 100
                0x12, 0x06,             // LabelTextUpdate (tag 2, len 6)
                0x08, 0x66,             // label_id = 102
                0x12, 0x02, 0x34, 0x35  // text = "45"
            ]
        );
    }

    #[test]
    fn test_encode_work_config_frame() {
        let frame = encode_work_config_frame("default_01.mp4.h264_2240x1080");
        assert_eq!(&frame[0..4], b"TRYX");
        // header (4 bytes) + field 200 (user_config)
        assert_eq!(&frame[8..12], &[0x0a, 0x02, 0x08, 0x01]);
        // field 200 tag (c2 0c)
        assert_eq!(&frame[12..14], &[0xc2, 0x0c]);
    }

    #[test]
    fn test_crc32c_standard_vectors() {
        assert_eq!(crc32c(b"123456789"), 0xe3069283);
        assert_eq!(crc32c(b""), 0x00000000);
    }

    #[test]
    fn test_file_transmit_framing_and_response() {
        let begin = encode_file_transmit_begin_frame("test.h264", 1024);
        assert_eq!(&begin[0..4], b"TRYX");
        // Tag 400 varint is (400 << 3) | 2 = 3202 = 0x82, 0x19
        assert_eq!(&begin[12..14], &[0x82, 0x19]);

        let data = encode_file_transmit_data_frame(&[1, 2, 3, 4]);
        assert_eq!(&data[0..4], b"TRYX");
        // Tag 401 varint is (401 << 3) | 2 = 3210 = 0x8a, 0x19
        assert_eq!(&data[12..14], &[0x8a, 0x19]);

        let end = encode_file_transmit_end_frame("media", 0x12345678);
        assert_eq!(&end[0..4], b"TRYX");
        // Tag 402 varint is (402 << 3) | 2 = 3218 = 0x92, 0x19
        assert_eq!(&end[12..14], &[0x92, 0x19]);

        // Verify status ACK decoding for tag 800/801/802
        let ack_800 = [
            0x54, 0x52, 0x59, 0x58, 0x09, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0x12, 0x00,
            0x82, 0x32, 0x00,
        ];
        let res = parse_response_frame(&ack_800).expect("valid tag 800 ack");
        assert!(res.ok);

        let err_802 = [
            0x54, 0x52, 0x59, 0x58, 0x0b, 0x00, 0x00, 0x00, 0x0a, 0x02, 0x08, 0x01, 0x12, 0x00,
            0x92, 0x32, 0x02, 0x08, 0x03,
        ];
        assert_eq!(
            parse_device_response(&err_802),
            Ok(DeviceResponse::Error { code: "CRCFail".to_string(), why: "".to_string() })
        );
    }
}
