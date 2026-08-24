/**
 * VirtioPacketBuilder - Encodes OASIS Virtio 1.2 GPU Binary Wire Protocol Packets
 */

export const VIRTIO_GPU_CMD = {
    GET_DISPLAY_INFO: 0x0100,
    RESOURCE_CREATE_2D: 0x0101,
    RESOURCE_UNREF: 0x0102,
    SET_SCANOUT: 0x0103,
    RESOURCE_FLUSH: 0x0104,
    TRANSFER_TO_HOST_2D: 0x0105,
    RESOURCE_ATTACH_BACKING: 0x0106,
    RESOURCE_DETACH_BACKING: 0x0107,
    GET_CAPSET_INFO: 0x0108,
    GET_CAPSET: 0x0109,
    GET_EDID: 0x010A,
    RESOURCE_ASSIGN_UUID: 0x010B,
    RESOURCE_CREATE_BLOB: 0x010C,
    SET_SCANOUT_BLOB: 0x010D,
    CTX_CREATE: 0x0200,
    CTX_DESTROY: 0x0201,
    CTX_ATTACH_RESOURCE: 0x0202,
    CTX_DETACH_RESOURCE: 0x0203,
    RESOURCE_CREATE_3D: 0x0204,
    TRANSFER_TO_HOST_3D: 0x0205,
    TRANSFER_FROM_HOST_3D: 0x0206,
    SUBMIT_3D: 0x0207,
};

export const VIRTIO_GPU_FORMAT = {
    B8G8R8A8_UNORM: 1,
    B8G8R8X8_UNORM: 2,
    A8R8G8B8_UNORM: 3,
    X8R8G8B8_UNORM: 4,
    R8G8B8A8_UNORM: 67,
    X8B8G8R8_UNORM: 68,
    A8B8G8R8_UNORM: 121,
    R8G8B8X8_UNORM: 134,
};

export class VirtioPacketBuilder {
    /**
     * Encode standard 24-byte Virtio Ctrl Header
     */
    static encodeHeader(type, flags = 0, fenceId = 0, ctxId = 0) {
        const buf = new ArrayBuffer(24);
        const view = new DataView(buf);
        view.setUint32(0, type, true);
        view.setUint32(4, flags, true);
        view.setBigUint64(8, BigInt(fenceId), true);
        view.setUint32(16, ctxId, true);
        view.setUint32(20, 0, true); // padding
        return new Uint8Array(buf);
    }

    /**
     * VIRTIO_GPU_CMD_RESOURCE_CREATE_2D
     */
    static createResource2d(resourceId, width, height, format = VIRTIO_GPU_FORMAT.R8G8B8A8_UNORM, fenceId = 1) {
        const buf = new ArrayBuffer(40);
        const view = new DataView(buf);
        const hdr = this.encodeHeader(VIRTIO_GPU_CMD.RESOURCE_CREATE_2D, 0, fenceId, 0);
        new Uint8Array(buf).set(hdr, 0);

        view.setUint32(24, resourceId, true);
        view.setUint32(28, format, true);
        view.setUint32(32, width, true);
        view.setUint32(36, height, true);
        return new Uint8Array(buf);
    }

    /**
     * VIRTIO_GPU_CMD_SET_SCANOUT
     */
    static setScanout(scanoutId, resourceId, width, height, x = 0, y = 0, fenceId = 2) {
        const buf = new ArrayBuffer(48);
        const view = new DataView(buf);
        const hdr = this.encodeHeader(VIRTIO_GPU_CMD.SET_SCANOUT, 0, fenceId, 0);
        new Uint8Array(buf).set(hdr, 0);

        // Rect (x, y, w, h)
        view.setUint32(24, x, true);
        view.setUint32(28, y, true);
        view.setUint32(32, width, true);
        view.setUint32(36, height, true);
        view.setUint32(40, scanoutId, true);
        view.setUint32(44, resourceId, true);
        return new Uint8Array(buf);
    }

    /**
     * VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D
     */
    static transferToHost2d(resourceId, width, height, x = 0, y = 0, pixelData = null, fenceId = 3) {
        const hdrSize = 56;
        const totalSize = hdrSize + (pixelData ? pixelData.length : 0);
        const buf = new ArrayBuffer(totalSize);
        const view = new DataView(buf);
        const hdr = this.encodeHeader(VIRTIO_GPU_CMD.TRANSFER_TO_HOST_2D, 0, fenceId, 0);
        new Uint8Array(buf).set(hdr, 0);

        // Rect (x, y, w, h)
        view.setUint32(24, x, true);
        view.setUint32(28, y, true);
        view.setUint32(32, width, true);
        view.setUint32(36, height, true);
        view.setBigUint64(40, 0n, true); // offset
        view.setUint32(48, resourceId, true);
        view.setUint32(52, 0, true); // padding

        if (pixelData) {
            new Uint8Array(buf).set(pixelData, hdrSize);
        }
        return new Uint8Array(buf);
    }

    /**
     * VIRTIO_GPU_CMD_RESOURCE_FLUSH
     */
    static resourceFlush(resourceId, width, height, x = 0, y = 0, fenceId = 4) {
        const buf = new ArrayBuffer(48);
        const view = new DataView(buf);
        const hdr = this.encodeHeader(VIRTIO_GPU_CMD.RESOURCE_FLUSH, 0, fenceId, 0);
        new Uint8Array(buf).set(hdr, 0);

        view.setUint32(24, x, true);
        view.setUint32(28, y, true);
        view.setUint32(32, width, true);
        view.setUint32(36, height, true);
        view.setUint32(40, resourceId, true);
        view.setUint32(44, 0, true); // padding
        return new Uint8Array(buf);
    }

    /**
     * VIRTIO_GPU_CMD_SUBMIT_3D
     */
    static submit3d(commandStreamBytes, fenceId = 5) {
        const hdrSize = 32;
        const totalSize = hdrSize + commandStreamBytes.length;
        const buf = new ArrayBuffer(totalSize);
        const view = new DataView(buf);
        const hdr = this.encodeHeader(VIRTIO_GPU_CMD.SUBMIT_3D, 0, fenceId, 0);
        new Uint8Array(buf).set(hdr, 0);

        view.setUint32(24, commandStreamBytes.length, true);
        view.setUint32(28, 0, true); // padding
        new Uint8Array(buf).set(commandStreamBytes, hdrSize);
        return new Uint8Array(buf);
    }

    /**
     * Encode GLES 3D Opcode Stream for SUBMIT_3D
     */
    static encodeGlesClear(mask = 0x4000, r = 0.0, g = 0.8, b = 0.3, a = 1.0) {
        const buf = new ArrayBuffer(28);
        const view = new DataView(buf);
        view.setUint32(0, 0x01, true); // Opcode: CLEAR
        view.setUint32(4, 20, true);   // Length: 20 bytes
        view.setUint32(8, mask, true);
        view.setFloat32(12, r, true);
        view.setFloat32(16, g, true);
        view.setFloat32(20, b, true);
        view.setFloat32(24, a, true);
        return new Uint8Array(buf);
    }

    static encodeGlesViewport(x = 0, y = 0, width = 640, height = 480) {
        const buf = new ArrayBuffer(24);
        const view = new DataView(buf);
        view.setUint32(0, 0x04, true); // Opcode: VIEWPORT
        view.setUint32(4, 16, true);   // Length: 16 bytes
        view.setInt32(8, x, true);
        view.setInt32(12, y, true);
        view.setUint32(16, width, true);
        view.setUint32(20, height, true);
        return new Uint8Array(buf);
    }

    static encodeGlesDrawArrays(mode = 0x0004, first = 0, count = 3) {
        const buf = new ArrayBuffer(20);
        const view = new DataView(buf);
        view.setUint32(0, 0x02, true); // Opcode: DRAW_ARRAYS
        view.setUint32(4, 12, true);   // Length: 12 bytes
        view.setUint32(8, mode, true);
        view.setUint32(12, first, true);
        view.setUint32(16, count, true);
        return new Uint8Array(buf);
    }
}
