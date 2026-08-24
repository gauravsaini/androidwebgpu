use crate::protocol::*;
use bytemuck::{bytes_of, try_from_bytes, Pod};

#[derive(Debug)]
pub enum DecodedVirtioCommand<'a> {
    GetDisplayInfo(VirtioGpuCtrlHdr),
    ResourceCreate2d(VirtioGpuResourceCreate2d),
    ResourceUnref(VirtioGpuCtrlHdr, u32),
    SetScanout(VirtioGpuSetScanout),
    ResourceFlush(VirtioGpuResourceFlush),
    TransferToHost2d(VirtioGpuTransferToHost2d, &'a [u8]),
    TransferToHost3d(VirtioGpuTransferToHost3d, &'a [u8]),
    ResourceAttachBacking(VirtioGpuResourceAttachBacking, Vec<VirtioGpuMemEntry>),
    CtxCreate(VirtioGpuCtxCreate),
    CtxDestroy(VirtioGpuCtrlHdr),
    CtxAttachResource(VirtioGpuCtxResource),
    CtxDetachResource(VirtioGpuCtxResource),
    ResourceCreate3d(VirtioGpuResourceCreate3d),
    GetCapsetInfo(VirtioGpuGetCapsetInfo),
    Submit3d(VirtioGpuSubmit3d, &'a [u8]),
    Unknown(VirtioGpuCtrlHdr),
}

pub struct BinaryWireParser;

impl BinaryWireParser {
    pub fn parse_command<'a>(bytes: &'a [u8]) -> Result<DecodedVirtioCommand<'a>, String> {
        let hdr_size = std::mem::size_of::<VirtioGpuCtrlHdr>();
        if bytes.len() < hdr_size {
            return Err("Packet too short for VirtioGpuCtrlHdr".to_string());
        }

        let hdr: &VirtioGpuCtrlHdr = try_from_bytes(&bytes[0..hdr_size])
            .map_err(|e| format!("Header parse failed: {:?}", e))?;

        match hdr.type_ {
            VIRTIO_GPU_CMD_GET_DISPLAY_INFO => Ok(DecodedVirtioCommand::GetDisplayInfo(*hdr)),
            VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                let size = std::mem::size_of::<VirtioGpuResourceCreate2d>();
                if bytes.len() < size {
                    return Err("Packet truncated for ResourceCreate2d".to_string());
                }
                let cmd: &VirtioGpuResourceCreate2d = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("ResourceCreate2d parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::ResourceCreate2d(*cmd))
            }
            VIRTIO_GPU_CMD_RESOURCE_UNREF => {
                if bytes.len() < hdr_size + 4 {
                    return Err("Packet truncated for ResourceUnref".to_string());
                }
                let res_id = u32::from_le_bytes([
                    bytes[hdr_size],
                    bytes[hdr_size + 1],
                    bytes[hdr_size + 2],
                    bytes[hdr_size + 3],
                ]);
                Ok(DecodedVirtioCommand::ResourceUnref(*hdr, res_id))
            }
            VIRTIO_GPU_CMD_SET_SCANOUT => {
                let size = std::mem::size_of::<VirtioGpuSetScanout>();
                if bytes.len() < size {
                    return Err("Packet truncated for SetScanout".to_string());
                }
                let cmd: &VirtioGpuSetScanout = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("SetScanout parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::SetScanout(*cmd))
            }
            VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
                let size = std::mem::size_of::<VirtioGpuResourceFlush>();
                if bytes.len() < size {
                    return Err("Packet truncated for ResourceFlush".to_string());
                }
                let cmd: &VirtioGpuResourceFlush = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("ResourceFlush parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::ResourceFlush(*cmd))
            }
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                let size = std::mem::size_of::<VirtioGpuTransferToHost2d>();
                if bytes.len() < size {
                    return Err("Packet truncated for TransferToHost2d".to_string());
                }
                let cmd: &VirtioGpuTransferToHost2d = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("TransferToHost2d parse error: {:?}", e))?;
                let payload = if bytes.len() > size { &bytes[size..] } else { &[] };
                Ok(DecodedVirtioCommand::TransferToHost2d(*cmd, payload))
            }
            VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D => {
                let size = std::mem::size_of::<VirtioGpuTransferToHost3d>();
                if bytes.len() < size {
                    return Err("Packet truncated for TransferToHost3d".to_string());
                }
                let cmd: &VirtioGpuTransferToHost3d = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("TransferToHost3d parse error: {:?}", e))?;
                let payload = if bytes.len() > size { &bytes[size..] } else { &[] };
                Ok(DecodedVirtioCommand::TransferToHost3d(*cmd, payload))
            }
            VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                let size = std::mem::size_of::<VirtioGpuResourceAttachBacking>();
                if bytes.len() < size {
                    return Err("Packet truncated for ResourceAttachBacking".to_string());
                }
                let cmd: &VirtioGpuResourceAttachBacking = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("ResourceAttachBacking parse error: {:?}", e))?;
                let entry_size = std::mem::size_of::<VirtioGpuMemEntry>();
                let mut entries = Vec::with_capacity(cmd.nr_entries as usize);
                let mut curr = size;
                for _ in 0..cmd.nr_entries {
                    if curr + entry_size <= bytes.len() {
                        if let Ok(entry) = try_from_bytes::<VirtioGpuMemEntry>(&bytes[curr..curr + entry_size]) {
                            entries.push(*entry);
                        }
                        curr += entry_size;
                    }
                }
                Ok(DecodedVirtioCommand::ResourceAttachBacking(*cmd, entries))
            }
            VIRTIO_GPU_CMD_CTX_CREATE => {
                let size = std::mem::size_of::<VirtioGpuCtxCreate>();
                if bytes.len() < size {
                    return Err("Packet truncated for CtxCreate".to_string());
                }
                let cmd: &VirtioGpuCtxCreate = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("CtxCreate parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::CtxCreate(*cmd))
            }
            VIRTIO_GPU_CMD_CTX_DESTROY => Ok(DecodedVirtioCommand::CtxDestroy(*hdr)),
            VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE => {
                let size = std::mem::size_of::<VirtioGpuCtxResource>();
                if bytes.len() < size {
                    return Err("Packet truncated for CtxAttachResource".to_string());
                }
                let cmd: &VirtioGpuCtxResource = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("CtxAttachResource parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::CtxAttachResource(*cmd))
            }
            VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE => {
                let size = std::mem::size_of::<VirtioGpuCtxResource>();
                if bytes.len() < size {
                    return Err("Packet truncated for CtxDetachResource".to_string());
                }
                let cmd: &VirtioGpuCtxResource = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("CtxDetachResource parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::CtxDetachResource(*cmd))
            }
            VIRTIO_GPU_CMD_RESOURCE_CREATE_3D => {
                let size = std::mem::size_of::<VirtioGpuResourceCreate3d>();
                if bytes.len() < size {
                    return Err("Packet truncated for ResourceCreate3d".to_string());
                }
                let cmd: &VirtioGpuResourceCreate3d = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("ResourceCreate3d parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::ResourceCreate3d(*cmd))
            }
            VIRTIO_GPU_CMD_GET_CAPSET_INFO => {
                let size = std::mem::size_of::<VirtioGpuGetCapsetInfo>();
                if bytes.len() < size {
                    return Err("Packet truncated for GetCapsetInfo".to_string());
                }
                let cmd: &VirtioGpuGetCapsetInfo = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("GetCapsetInfo parse error: {:?}", e))?;
                Ok(DecodedVirtioCommand::GetCapsetInfo(*cmd))
            }
            VIRTIO_GPU_CMD_SUBMIT_3D => {
                let size = std::mem::size_of::<VirtioGpuSubmit3d>();
                if bytes.len() < size {
                    return Err("Packet truncated for Submit3d".to_string());
                }
                let cmd: &VirtioGpuSubmit3d = try_from_bytes(&bytes[0..size])
                    .map_err(|e| format!("Submit3d parse error: {:?}", e))?;
                let payload = if bytes.len() > size { &bytes[size..] } else { &[] };
                Ok(DecodedVirtioCommand::Submit3d(*cmd, payload))
            }
            _ => Ok(DecodedVirtioCommand::Unknown(*hdr)),
        }
    }

    pub fn encode_header_response(type_: u32, fence_id: u64, flags: u32) -> Vec<u8> {
        let hdr = VirtioGpuCtrlHdr {
            type_,
            flags,
            fence_id,
            ctx_id: 0,
            padding: 0,
        };
        bytes_of(&hdr).to_vec()
    }

    pub fn encode_response<T: Pod>(resp: &T) -> Vec<u8> {
        bytes_of(resp).to_vec()
    }
}
