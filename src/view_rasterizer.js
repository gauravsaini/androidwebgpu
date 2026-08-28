/**
 * AndroidWebGPU - Authentic Android View Rasterizer & Canvas Renderer
 * 
 * Provides:
 * 1. ViewHierarchyRasterizer: Hardware / 2D Canvas rasterizer for in-memory Android View hierarchies.
 * 2. MotionEvent & KeyEvent: Android input event models.
 * 3. ViewRootImpl: In-memory window tree root managing measure/layout/draw passes and input dispatch.
 * 4. ActivityBackstack: Android Activity backstack manager.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { MeasureSpec, LayoutParams, View, ViewGroup, FrameLayout, LinearLayout, RelativeLayout, ConstraintLayout, ScrollView, RecyclerView, TextView, ImageView, Button, LayoutInflater, VISIBLE } from './view_hierarchy.js';
import { VirtioPacketBuilder } from './virtio_packet_builder.js';

// -----------------------------------------------------------------------------
// 1. Input Event Models
// -----------------------------------------------------------------------------

export class MotionEvent {
    static ACTION_DOWN = 0;
    static ACTION_UP = 1;
    static ACTION_MOVE = 2;
    static ACTION_CANCEL = 3;
    static ACTION_SCROLL = 8;

    constructor(action, x, y, eventTime = Date.now()) {
        this.action = action;
        this.x = x;
        this.y = y;
        this.eventTime = eventTime;
        this.pointerCount = 1;
    }
}

export class KeyEvent {
    static ACTION_DOWN = 0;
    static ACTION_UP = 1;
    static KEYCODE_BACK = 4;
    static KEYCODE_HOME = 3;
    static KEYCODE_APP_SWITCH = 187;

    constructor(action, keyCode, eventTime = Date.now()) {
        this.action = action;
        this.keyCode = keyCode;
        this.eventTime = eventTime;
    }
}

// -----------------------------------------------------------------------------
// 2. ViewRootImpl (Window Root Bridge)
// -----------------------------------------------------------------------------

export class ViewRootImpl {
    constructor(canvas = null) {
        this.rootView = null;
        this.canvas = canvas;
        this.ctx = canvas ? canvas.getContext('2d') : null;
        this.width = canvas ? canvas.width : 720;
        this.height = canvas ? canvas.height : 1440;
        this.isDirty = true;
    }

    setView(view) {
        this.rootView = view;
        this.isDirty = true;
        this.performTraversals();
    }

    getRootView() {
        return this.rootView;
    }

    setCanvas(canvas) {
        this.canvas = canvas;
        this.ctx = canvas ? canvas.getContext('2d') : null;
        if (canvas) {
            this.width = canvas.width;
            this.height = canvas.height;
        }
        this.isDirty = true;
    }

    performTraversals() {
        if (!this.rootView) return;
        const wSpec = MeasureSpec.makeMeasureSpec(this.width, MeasureSpec.EXACTLY);
        const hSpec = MeasureSpec.makeMeasureSpec(this.height, MeasureSpec.EXACTLY);
        this.rootView.measure(wSpec, hSpec);
        this.rootView.layout(0, 0, this.width, this.height);
        this.draw();
        if (typeof window !== 'undefined' && window.logDebug) {
            window.logDebug('runtime', 'D', `[ViewRootImpl] Traversal pass: Root=${this.rootView.constructor.name}, Bounds=${this.width}x${this.height}`);
        }
    }

    draw() {
        if (!this.rootView || !this.ctx) return;
        this.ctx.save();
        this.ctx.clearRect(0, 0, this.width, this.height);

        // Draw Android System App Background
        this.ctx.fillStyle = '#0f172a';
        this.ctx.fillRect(0, 0, this.width, this.height);

        // Draw View Hierarchy
        this.rootView.draw(this.ctx);
        this.ctx.restore();
        this.isDirty = false;
    }

    dispatchInputEvent(event) {
        if (!this.rootView) return false;
        if (event instanceof MotionEvent) {
            const handled = this.rootView.dispatchTouchEvent(event);
            this.draw();
            // Strategically log discrete actions (DOWN, UP, SCROLL), ignore high-frequency MOVE
            if (event.action !== MotionEvent.ACTION_MOVE && typeof window !== 'undefined' && window.logDebug) {
                const actName = event.action === MotionEvent.ACTION_DOWN ? 'ACTION_DOWN' 
                    : (event.action === MotionEvent.ACTION_UP ? 'ACTION_UP' 
                    : (event.action === MotionEvent.ACTION_SCROLL ? 'ACTION_SCROLL' : 'ACTION_OTHER'));
                window.logDebug('input', 'D', `[InputDispatcher] ${actName} at (${event.x}, ${event.y}) -> Handled: ${handled}`);
            }
            return handled;
        }
        return false;
    }
}

// -----------------------------------------------------------------------------
// 3. Activity Backstack Manager
// -----------------------------------------------------------------------------

export class ActivityBackstack {
    constructor() {
        this.stack = [];
    }

    push(activity) {
        this.stack.push(activity);
    }

    pop() {
        return this.stack.pop() || null;
    }

    top() {
        return this.stack.length > 0 ? this.stack[this.stack.length - 1] : null;
    }

    size() {
        return this.stack.length;
    }

    clear() {
        this.stack = [];
    }
}

// -----------------------------------------------------------------------------
export function parseCssColor(str) {
    if (!str || typeof str !== 'string') return [0, 0, 0, 255];
    str = str.trim();
    if (str.startsWith('#')) {
        const hex = str.slice(1);
        if (hex.length === 3) {
            return [
                parseInt(hex[0] + hex[0], 16),
                parseInt(hex[1] + hex[1], 16),
                parseInt(hex[2] + hex[2], 16),
                255
            ];
        } else if (hex.length === 4) {
            return [
                parseInt(hex[0] + hex[0], 16),
                parseInt(hex[1] + hex[1], 16),
                parseInt(hex[2] + hex[2], 16),
                parseInt(hex[3] + hex[3], 16)
            ];
        } else if (hex.length === 6) {
            return [
                parseInt(hex.slice(0, 2), 16),
                parseInt(hex.slice(2, 4), 16),
                parseInt(hex.slice(4, 6), 16),
                255
            ];
        } else if (hex.length === 8) {
            return [
                parseInt(hex.slice(0, 2), 16),
                parseInt(hex.slice(2, 4), 16),
                parseInt(hex.slice(4, 6), 16),
                parseInt(hex.slice(6, 8), 16)
            ];
        }
    } else if (str.startsWith('rgb')) {
        const m = str.match(/rgba?\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*([\d.]+))?\s*\)/);
        if (m) {
            const r = Math.min(255, parseInt(m[1], 10));
            const g = Math.min(255, parseInt(m[2], 10));
            const b = Math.min(255, parseInt(m[3], 10));
            const a = m[4] !== undefined ? Math.min(255, Math.max(0, Math.round(parseFloat(m[4]) * 255))) : 255;
            return [r, g, b, a];
        }
    } else if (str === 'transparent') {
        return [0, 0, 0, 0];
    } else if (str === 'white') {
        return [255, 255, 255, 255];
    } else if (str === 'black') {
        return [0, 0, 0, 255];
    }
    return [0, 0, 0, 255];
}

export const FONT_5X7 = new Uint8Array([
    0x00,0x00,0x00,0x00,0x00, 0x00,0x00,0x5f,0x00,0x00, 0x00,0x07,0x00,0x07,0x00, 0x14,0x7f,0x14,0x7f,0x14,
    0x24,0x2a,0x7f,0x2a,0x12, 0x23,0x13,0x08,0x64,0x62, 0x36,0x49,0x55,0x22,0x50, 0x00,0x05,0x03,0x00,0x00,
    0x00,0x1c,0x22,0x41,0x00, 0x00,0x41,0x22,0x1c,0x00, 0x14,0x08,0x3e,0x08,0x14, 0x08,0x08,0x3e,0x08,0x08,
    0x00,0x50,0x30,0x00,0x00, 0x08,0x08,0x08,0x08,0x08, 0x00,0x60,0x60,0x00,0x00, 0x20,0x10,0x08,0x04,0x02,
    0x3e,0x51,0x49,0x45,0x3e, 0x00,0x42,0x7f,0x40,0x00, 0x42,0x61,0x51,0x49,0x46, 0x21,0x41,0x45,0x4b,0x31,
    0x18,0x14,0x12,0x7f,0x10, 0x27,0x45,0x45,0x45,0x39, 0x3c,0x4a,0x49,0x49,0x30, 0x01,0x71,0x09,0x05,0x03,
    0x36,0x49,0x49,0x49,0x36, 0x06,0x49,0x49,0x29,0x1e, 0x00,0x36,0x36,0x00,0x00, 0x00,0x56,0x36,0x00,0x00,
    0x08,0x14,0x22,0x41,0x00, 0x14,0x14,0x14,0x14,0x14, 0x00,0x41,0x22,0x14,0x08, 0x02,0x01,0x51,0x09,0x06,
    0x32,0x49,0x79,0x41,0x3e, 0x7e,0x11,0x11,0x11,0x7e, 0x7f,0x49,0x49,0x49,0x36, 0x3e,0x41,0x41,0x41,0x22,
    0x7f,0x41,0x41,0x22,0x1c, 0x7f,0x49,0x49,0x49,0x41, 0x7f,0x09,0x09,0x09,0x01, 0x3e,0x41,0x49,0x49,0x7a,
    0x7f,0x08,0x08,0x08,0x7f, 0x00,0x41,0x7f,0x41,0x00, 0x20,0x40,0x41,0x3f,0x01, 0x7f,0x08,0x14,0x22,0x41,
    0x7f,0x40,0x40,0x40,0x40, 0x7f,0x02,0x0c,0x02,0x7f, 0x7f,0x04,0x08,0x10,0x7f, 0x3e,0x41,0x41,0x41,0x3e,
    0x7f,0x09,0x09,0x09,0x06, 0x3e,0x41,0x51,0x21,0x5e, 0x7f,0x09,0x19,0x29,0x46, 0x46,0x49,0x49,0x49,0x31,
    0x01,0x01,0x7f,0x01,0x01, 0x3f,0x40,0x40,0x40,0x3f, 0x1f,0x20,0x40,0x20,0x1f, 0x3f,0x40,0x38,0x40,0x3f,
    0x63,0x14,0x08,0x14,0x63, 0x07,0x08,0x70,0x08,0x07, 0x61,0x51,0x49,0x45,0x43, 0x00,0x7f,0x41,0x41,0x00,
    0x02,0x04,0x08,0x10,0x20, 0x00,0x41,0x41,0x7f,0x00, 0x04,0x02,0x01,0x02,0x04, 0x40,0x40,0x40,0x40,0x40,
    0x00,0x01,0x02,0x04,0x00, 0x20,0x54,0x54,0x54,0x78, 0x7f,0x48,0x44,0x44,0x38, 0x38,0x44,0x44,0x44,0x20,
    0x38,0x44,0x44,0x48,0x7f, 0x38,0x54,0x54,0x54,0x18, 0x08,0x7e,0x09,0x01,0x02, 0x0c,0x52,0x52,0x52,0x3e,
    0x7f,0x08,0x04,0x04,0x78, 0x00,0x44,0x7d,0x40,0x00, 0x20,0x40,0x44,0x3d,0x00, 0x7f,0x10,0x28,0x44,0x00,
    0x00,0x41,0x7f,0x40,0x00, 0x7c,0x04,0x18,0x04,0x78, 0x7c,0x08,0x04,0x04,0x78, 0x38,0x44,0x44,0x44,0x38,
    0x7c,0x14,0x14,0x14,0x08, 0x08,0x14,0x14,0x18,0x7c, 0x7c,0x08,0x04,0x04,0x08, 0x48,0x54,0x54,0x54,0x20,
    0x04,0x3f,0x44,0x40,0x20, 0x3c,0x40,0x40,0x20,0x7c, 0x1c,0x20,0x40,0x20,0x1c, 0x3c,0x40,0x30,0x40,0x3c,
    0x44,0x28,0x10,0x28,0x44, 0x0c,0x50,0x50,0x50,0x3c, 0x44,0x64,0x54,0x4c, 0x44, 0x00,0x08,0x36,0x41,0x00,
    0x00,0x00,0x7f,0x00,0x00, 0x00,0x41,0x36,0x08,0x00, 0x08,0x08,0x2a,0x10,0x10
]);

export class Software2DContext {
    constructor(rgbaData, width, height) {
        this.rgbaData = rgbaData;
        this.width = width;
        this.height = height;
        this.fillStyle = '#000000';
        this.strokeStyle = '#000000';
        this.font = '14px sans-serif';
        this.textAlign = 'start';
        this.textBaseline = 'middle';
        this.globalAlpha = 1.0;
        this.transX = 0;
        this.transY = 0;
        this.clipRect = null;
        this.currentPathRect = null;
        this.stateStack = [];
    }

    save() {
        this.stateStack.push({
            fillStyle: this.fillStyle,
            strokeStyle: this.strokeStyle,
            font: this.font,
            textAlign: this.textAlign,
            textBaseline: this.textBaseline,
            globalAlpha: this.globalAlpha,
            transX: this.transX,
            transY: this.transY,
            clipRect: this.clipRect ? [...this.clipRect] : null
        });
    }

    restore() {
        if (this.stateStack.length === 0) return;
        const s = this.stateStack.pop();
        this.fillStyle = s.fillStyle;
        this.strokeStyle = s.strokeStyle;
        this.font = s.font;
        this.textAlign = s.textAlign;
        this.textBaseline = s.textBaseline;
        this.globalAlpha = s.globalAlpha;
        this.transX = s.transX;
        this.transY = s.transY;
        this.clipRect = s.clipRect;
    }

    translate(x, y) {
        this.transX += x;
        this.transY += y;
    }

    beginPath() {
        this.currentPathRect = null;
    }

    closePath() {}

    rect(x, y, w, h) {
        this.currentPathRect = [
            x + this.transX,
            y + this.transY,
            x + this.transX + w,
            y + this.transY + h
        ];
    }

    roundRect(x, y, w, h, r = 0) {
        this.rect(x, y, w, h);
    }

    clip() {
        if (!this.currentPathRect) return;
        if (!this.clipRect) {
            this.clipRect = [...this.currentPathRect];
        } else {
            this.clipRect = [
                Math.max(this.clipRect[0], this.currentPathRect[0]),
                Math.max(this.clipRect[1], this.currentPathRect[1]),
                Math.min(this.clipRect[2], this.currentPathRect[2]),
                Math.min(this.clipRect[3], this.currentPathRect[3])
            ];
        }
    }

    fill() {
        if (this.currentPathRect) {
            const [x0, y0, x1, y1] = this.currentPathRect;
            this.fillRect(x0 - this.transX, y0 - this.transY, x1 - x0, y1 - y0);
        }
    }

    stroke() {}

    strokeRect(x, y, w, h) {}

    fillRect(x, y, w, h) {
        const color = parseCssColor(this.fillStyle);
        const alpha = (color[3] / 255) * this.globalAlpha;
        if (alpha <= 0 || w <= 0 || h <= 0) return;

        const rx = Math.round(x + this.transX);
        const ry = Math.round(y + this.transY);
        const rw = Math.round(w);
        const rh = Math.round(h);

        const clipX0 = this.clipRect ? this.clipRect[0] : 0;
        const clipY0 = this.clipRect ? this.clipRect[1] : 0;
        const clipX1 = this.clipRect ? this.clipRect[2] : this.width;
        const clipY1 = this.clipRect ? this.clipRect[3] : this.height;

        const sx = Math.max(0, Math.max(clipX0, rx));
        const sy = Math.max(0, Math.max(clipY0, ry));
        const ex = Math.min(this.width, Math.min(clipX1, rx + rw));
        const ey = Math.min(this.height, Math.min(clipY1, ry + rh));

        if (sx >= ex || sy >= ey) return;

        const [sr, sg, sb] = color;

        if (alpha >= 0.999) {
            for (let r = sy; r < ey; r++) {
                let idx = (r * this.width + sx) * 4;
                for (let c = sx; c < ex; c++) {
                    this.rgbaData[idx] = sr;
                    this.rgbaData[idx + 1] = sg;
                    this.rgbaData[idx + 2] = sb;
                    this.rgbaData[idx + 3] = 255;
                    idx += 4;
                }
            }
        } else {
            const invA = 1 - alpha;
            for (let r = sy; r < ey; r++) {
                let idx = (r * this.width + sx) * 4;
                for (let c = sx; c < ex; c++) {
                    const dr = this.rgbaData[idx];
                    const dg = this.rgbaData[idx + 1];
                    const db = this.rgbaData[idx + 2];
                    const da = this.rgbaData[idx + 3] / 255;

                    const outA = alpha + da * invA;
                    const norm = outA > 0 ? outA : 1;
                    this.rgbaData[idx] = Math.round((sr * alpha + dr * da * invA) / norm);
                    this.rgbaData[idx + 1] = Math.round((sg * alpha + dg * da * invA) / norm);
                    this.rgbaData[idx + 2] = Math.round((sb * alpha + db * da * invA) / norm);
                    this.rgbaData[idx + 3] = Math.round(outA * 255);
                    idx += 4;
                }
            }
        }
    }

    clearRect(x, y, w, h) {
        const oldFill = this.fillStyle;
        this.fillStyle = 'rgba(0,0,0,0)';
        this.fillRect(x, y, w, h);
        this.fillStyle = oldFill;
    }

    measureText(text) {
        const str = String(text || '');
        const sizeMatch = (this.font || '').match(/(\d+)px/);
        const fontSize = sizeMatch ? parseInt(sizeMatch[1], 10) : 14;
        const scale = Math.max(1, Math.round(fontSize / 7));
        const charW = scale * 6;
        return {
            width: str.length * charW,
            actualBoundingBoxAscent: scale * 6,
            actualBoundingBoxDescent: scale * 2
        };
    }

    fillText(text, x, y, maxW) {
        if (!text) return;
        const str = String(text);
        const sizeMatch = (this.font || '').match(/(\d+)px/);
        const fontSize = sizeMatch ? parseInt(sizeMatch[1], 10) : 14;
        const scale = Math.max(1, Math.round(fontSize / 7));
        const charW = scale * 6;
        const charH = scale * 8;

        const fullW = str.length * charW;
        const textW = (maxW !== undefined && maxW !== null && maxW > 0) ? Math.min(maxW, fullW) : fullW;

        let tx = x + this.transX;
        if (this.textAlign === 'center') {
            tx -= textW / 2;
        } else if (this.textAlign === 'right' || this.textAlign === 'end') {
            tx -= textW;
        }

        let ty = y + this.transY;
        if (this.textBaseline === 'middle') {
            ty -= charH / 2;
        } else if (this.textBaseline === 'bottom') {
            ty -= charH;
        } else if (this.textBaseline === 'alphabetic') {
            ty -= scale * 6;
        }

        const color = parseCssColor(this.fillStyle);
        const alpha = (color[3] / 255) * this.globalAlpha;
        if (alpha <= 0) return;

        const [sr, sg, sb] = color;
        const clipX0 = this.clipRect ? this.clipRect[0] : 0;
        const clipY0 = this.clipRect ? this.clipRect[1] : 0;
        const clipX1 = this.clipRect ? this.clipRect[2] : this.width;
        const clipY1 = this.clipRect ? this.clipRect[3] : this.height;

        let curX = Math.round(tx);
        const startY = Math.round(ty);

        for (let i = 0; i < str.length; i++) {
            if (maxW && (curX - Math.round(tx) + charW) > maxW) break;
            const code = str.charCodeAt(i);
            const glyphIdx = (code >= 32 && code <= 126) ? (code - 32) * 5 : (63 - 32) * 5;

            for (let col = 0; col < 5; col++) {
                const colBits = FONT_5X7[glyphIdx + col];
                for (let row = 0; row < 7; row++) {
                    if ((colBits & (1 << row)) !== 0) {
                        for (let sy = 0; sy < scale; sy++) {
                            const py = startY + row * scale + sy;
                            if (py < clipY0 || py >= clipY1 || py < 0 || py >= this.height) continue;
                            for (let sx = 0; sx < scale; sx++) {
                                const px = curX + col * scale + sx;
                                if (px < clipX0 || px >= clipX1 || px < 0 || px >= this.width) continue;

                                const idx = (py * this.width + px) * 4;
                                if (alpha >= 0.999) {
                                    this.rgbaData[idx] = sr;
                                    this.rgbaData[idx + 1] = sg;
                                    this.rgbaData[idx + 2] = sb;
                                    this.rgbaData[idx + 3] = 255;
                                } else {
                                    const invA = 1 - alpha;
                                    const dr = this.rgbaData[idx];
                                    const dg = this.rgbaData[idx + 1];
                                    const db = this.rgbaData[idx + 2];
                                    const da = this.rgbaData[idx + 3] / 255;
                                    const outA = alpha + da * invA;
                                    const norm = outA > 0 ? outA : 1;
                                    this.rgbaData[idx] = Math.round((sr * alpha + dr * da * invA) / norm);
                                    this.rgbaData[idx + 1] = Math.round((sg * alpha + dg * da * invA) / norm);
                                    this.rgbaData[idx + 2] = Math.round((sb * alpha + db * da * invA) / norm);
                                    this.rgbaData[idx + 3] = Math.round(outA * 255);
                                }
                            }
                        }
                    }
                }
            }
            curX += charW;
        }
    }

    strokeText(text, x, y, maxW) {}

    drawImage() {}
}

// -----------------------------------------------------------------------------
// 4. ViewHierarchyRasterizer
// -----------------------------------------------------------------------------

export class ViewHierarchyRasterizer {
    constructor(width = 720, height = 1440) {
        this.width = width;
        this.height = height;
        this.rgbaData = new Uint8Array(width * height * 4);
    }

    /**
     * Measures, positions, and rasterizes a View tree into an RGBA pixel buffer.
     * @param {View} rootView
     * @param {number} [width]
     * @param {number} [height]
     * @returns {{ width: number, height: number, rgbaData: Uint8Array, damageRect: number[] }}
     */
    rasterize(rootView, width = this.width, height = this.height) {
        this.width = width;
        this.height = height;
        const totalBytes = width * height * 4;
        if (this.rgbaData.length !== totalBytes) {
            this.rgbaData = new Uint8Array(totalBytes);
        }

        // Material Design 3 Dark Surface (#0f172a -> R:15, G:23, B:42, A:255)
        for (let i = 0; i < totalBytes; i += 4) {
            this.rgbaData[i] = 15;
            this.rgbaData[i + 1] = 23;
            this.rgbaData[i + 2] = 42;
            this.rgbaData[i + 3] = 255;
        }

        if (!rootView) {
            return { width, height, rgbaData: this.rgbaData, damageRect: [0, 0, width, height] };
        }

        rootView.measure(
            MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY),
            MeasureSpec.makeMeasureSpec(height, MeasureSpec.EXACTLY)
        );
        rootView.layout(0, 0, width, height);

        let renderedWithCanvas = false;
        if (typeof OffscreenCanvas !== 'undefined') {
            try {
                const offscreen = new OffscreenCanvas(width, height);
                const ctx = offscreen.getContext('2d');
                if (ctx) {
                    ctx.fillStyle = '#0f172a';
                    ctx.fillRect(0, 0, width, height);
                    rootView.draw(ctx);
                    const imgData = ctx.getImageData(0, 0, width, height);
                    this.rgbaData.set(imgData.data);
                    renderedWithCanvas = true;
                }
            } catch (_) {}
        } else if (typeof document !== 'undefined' && typeof document.createElement === 'function') {
            try {
                const domCanvas = document.createElement('canvas');
                domCanvas.width = width;
                domCanvas.height = height;
                const ctx = domCanvas.getContext('2d');
                if (ctx) {
                    ctx.fillStyle = '#0f172a';
                    ctx.fillRect(0, 0, width, height);
                    rootView.draw(ctx);
                    const imgData = ctx.getImageData(0, 0, width, height);
                    this.rgbaData.set(imgData.data);
                    renderedWithCanvas = true;
                }
            } catch (_) {}
        }

        if (!renderedWithCanvas) {
            const ctx = new Software2DContext(this.rgbaData, width, height);
            rootView.draw(ctx);
        }

        return {
            width,
            height,
            rgbaData: this.rgbaData,
            damageRect: [0, 0, width, height]
        };
    }

    /**
     * Submits rasterized buffer to VirtIO GPU device control queue.
     */
    submitToVirtioGpu(device, resId = 100, scanoutId = 0, buffer = this.rgbaData) {
        if (!device) return;
        if (typeof device.isHostInjectionAllowed === 'function' && !device.isHostInjectionAllowed()) return;
        if (device.guestActive || device.hostInjectionBlocked) return;
        const transferPkt = VirtioPacketBuilder.transferToHost2d(resId, this.width, this.height, 0, 0, buffer);
        device.processControlQueue(transferPkt);
        const flushPkt = VirtioPacketBuilder.resourceFlush(resId, this.width, this.height, 0, 0);
        device.processControlQueue(flushPkt);
    }
}

if (typeof window !== 'undefined') {
    window.MotionEvent = MotionEvent;
    window.KeyEvent = KeyEvent;
    window.ViewRootImpl = ViewRootImpl;
    window.ViewHierarchyRasterizer = ViewHierarchyRasterizer;
    window.ActivityBackstack = ActivityBackstack;
}

