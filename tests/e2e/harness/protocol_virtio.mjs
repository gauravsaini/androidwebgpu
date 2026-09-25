/**
 * Virtio Device Protocol Specifications & Wire Codecs
 * Implements specifications for F-DEV-01 through F-DEV-07 and F-GPU-01/02/05.
 */

// Virtio-Blk
export const VIRTIO_BLK_T_IN = 0;
export const VIRTIO_BLK_T_OUT = 1;
export const VIRTIO_BLK_T_FLUSH = 4;
export const VIRTIO_BLK_S_OK = 0;
export const VIRTIO_BLK_S_IOERR = 1;
export const VIRTIO_BLK_S_UNSUPP = 2;

export function encodeVirtioBlkReq(type, sector, iovLength) {
  const buf = new Uint8Array(16);
  const view = new DataView(buf.buffer);
  view.setUint32(0, type, true);
  view.setUint32(4, 0, true); // reserved / priority
  const secLo = Number(BigInt(sector) & 0xFFFFFFFFn);
  const secHi = Number((BigInt(sector) >> 32n) & 0xFFFFFFFFn);
  view.setUint32(8, secLo, true);
  view.setUint32(12, secHi, true);
  return buf;
}

export function decodeVirtioBlkReq(buffer) {
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  const type = view.getUint32(0, true);
  const secLo = view.getUint32(8, true);
  const secHi = view.getUint32(12, true);
  const sector = BigInt(secLo) | (BigInt(secHi) << 32n);
  return { type, sector };
}

// Virtio-Input
export const EV_SYN = 0x00;
export const EV_KEY = 0x01;
export const EV_REL = 0x02;
export const EV_ABS = 0x03;

export const SYN_REPORT = 0;
export const BTN_TOUCH = 0x14A;
export const ABS_X = 0x00;
export const ABS_Y = 0x01;
export const ABS_MT_SLOT = 0x2F;
export const ABS_MT_TOUCH_MAJOR = 0x30;
export const ABS_MT_POSITION_X = 0x35;
export const ABS_MT_POSITION_Y = 0x36;
export const ABS_MT_TRACKING_ID = 0x39;

export function encodeVirtioInputEvent(type, code, value) {
  const buf = new Uint8Array(8);
  const view = new DataView(buf.buffer);
  view.setUint16(0, type, true);
  view.setUint16(2, code, true);
  view.setUint32(4, value >>> 0, true);
  return buf;
}

export function decodeVirtioInputEvent(buf) {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  return {
    type: view.getUint16(0, true),
    code: view.getUint16(2, true),
    value: view.getUint32(4, true)
  };
}

// Virtio-Net
export const VIRTIO_NET_HDR_F_NEEDS_CSUM = 1;
export const VIRTIO_NET_HDR_GSO_NONE = 0;
export const VIRTIO_NET_HDR_GSO_TCPV4 = 1;
export const VIRTIO_NET_HDR_GSO_UDP = 3;
export const VIRTIO_NET_HDR_GSO_TCPV6 = 4;

export function encodeVirtioNetHeader({ flags = 0, gsoType = 0, hdrLen = 0, gsoSize = 0, csumStart = 0, csumOffset = 0, numBuffers = 1 } = {}) {
  const buf = new Uint8Array(12);
  const view = new DataView(buf.buffer);
  view.setUint8(0, flags);
  view.setUint8(1, gsoType);
  view.setUint16(2, hdrLen, true);
  view.setUint16(4, gsoSize, true);
  view.setUint16(6, csumStart, true);
  view.setUint16(8, csumOffset, true);
  view.setUint16(10, numBuffers, true);
  return buf;
}

export function decodeVirtioNetHeader(buf) {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  return {
    flags: view.getUint8(0),
    gsoType: view.getUint8(1),
    hdrLen: view.getUint16(2, true),
    gsoSize: view.getUint16(4, true),
    csumStart: view.getUint16(6, true),
    csumOffset: view.getUint16(8, true),
    numBuffers: view.getUint16(10, true)
  };
}

// Virtio-GPU
export const VIRTIO_GPU_CMD_GET_DISPLAY_INFO = 0x0100;
export const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D = 0x0101;
export const VIRTIO_GPU_CMD_RESOURCE_UNREF = 0x0102;
export const VIRTIO_GPU_CMD_SET_SCANOUT = 0x0103;
export const VIRTIO_GPU_CMD_RESOURCE_FLUSH = 0x0104;
export const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D = 0x0105;
export const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING = 0x0106;
export const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING = 0x0107;
export const VIRTIO_GPU_CMD_SUBMIT_3D = 0x0108;

export const VIRTIO_GPU_RESP_OK_NODATA = 0x1100;
export const VIRTIO_GPU_RESP_OK_DISPLAY_INFO = 0x1101;
export const VIRTIO_GPU_RESP_ERR_UNSPEC = 0x1200;
export const VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY = 0x1201;
export const VIRTIO_GPU_RESP_ERR_INVALID_SCANOUT_ID = 0x1202;
export const VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID = 0x1203;
export const VIRTIO_GPU_RESP_ERR_INVALID_CONTEXT_ID = 0x1204;
export const VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER = 0x1205;

export const VIRTIO_GPU_FLAG_FENCE = 0x01;

export const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM = 1;
export const VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM = 2;
export const VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM = 67;

export function encodeVirtioGpuHeader({ type, flags = 0, fenceId = 0n, ctxId = 0 }) {
  const buf = new Uint8Array(24);
  const view = new DataView(buf.buffer);
  view.setUint32(0, type, true);
  view.setUint32(4, flags, true);
  const fenceLo = Number(BigInt(fenceId) & 0xFFFFFFFFn);
  const fenceHi = Number((BigInt(fenceId) >> 32n) & 0xFFFFFFFFn);
  view.setUint32(8, fenceLo, true);
  view.setUint32(12, fenceHi, true);
  view.setUint32(16, ctxId, true);
  view.setUint32(20, 0, true); // padding
  return buf;
}

export function decodeVirtioGpuHeader(buf) {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const type = view.getUint32(0, true);
  const flags = view.getUint32(4, true);
  const fenceLo = view.getUint32(8, true);
  const fenceHi = view.getUint32(12, true);
  const fenceId = BigInt(fenceLo) | (BigInt(fenceHi) << 32n);
  const ctxId = view.getUint32(16, true);
  return { type, flags, fenceId, ctxId };
}
