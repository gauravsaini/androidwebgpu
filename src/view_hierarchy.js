/**
 * AndroidWebGPU - Authentic Android View Hierarchy & Binary XML Layout Inflater
 * 
 * Provides:
 * 1. MeasureSpec: Bitmask spec (UNSPECIFIED, EXACTLY, AT_MOST) and size unpackers.
 * 2. LayoutParams: width, height (MATCH_PARENT=-1, WRAP_CONTENT=-2), margins, weights, alignment & constraint rules.
 * 3. View: Base view model with measure, layout, draw, touch event dispatch, and click listeners.
 * 4. ViewGroup: Container view with child management, reverse-Z hit testing, and measure algorithms.
 * 5. Layout Containers: FrameLayout, LinearLayout, RelativeLayout, ConstraintLayout, ScrollView, RecyclerView/ListView.
 * 6. Core Widgets: TextView, ImageView, Button.
 * 7. LayoutInflater: Binary XML AST inflation, resource resolution, and View tree construction.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { TypedValue } from './apk_resource_resolver.js';

// -----------------------------------------------------------------------------
// 1. MeasureSpec
// -----------------------------------------------------------------------------

export const MODE_SHIFT = 30;
export const MODE_MASK = (0x3 << MODE_SHIFT) >>> 0;
export const UNSPECIFIED = (0 << MODE_SHIFT) >>> 0;
export const EXACTLY = (1 << MODE_SHIFT) >>> 0;
export const AT_MOST = (2 << MODE_SHIFT) >>> 0;

export class MeasureSpec {
    static UNSPECIFIED = UNSPECIFIED;
    static EXACTLY = EXACTLY;
    static AT_MOST = AT_MOST;
    static MODE_MASK = MODE_MASK;

    /**
     * Creates a measure specification from size and mode.
     * @param {number} size - Desired pixel size.
     * @param {number} mode - UNSPECIFIED (0), EXACTLY (0x40000000), or AT_MOST (0x80000000).
     * @returns {number} 32-bit measure specification.
     */
    static makeMeasureSpec(size, mode) {
        return (((size >>> 0) & ~MODE_MASK) | (mode & MODE_MASK)) >>> 0;
    }

    /**
     * Extracts mode from measure specification.
     * @param {number} measureSpec
     * @returns {number}
     */
    static getMode(measureSpec) {
        return (measureSpec & MODE_MASK) >>> 0;
    }

    /**
     * Extracts size in pixels from measure specification.
     * @param {number} measureSpec
     * @returns {number}
     */
    static getSize(measureSpec) {
        return (measureSpec & ~MODE_MASK) >>> 0;
    }

    static toString(measureSpec) {
        const mode = MeasureSpec.getMode(measureSpec);
        const size = MeasureSpec.getSize(measureSpec);
        let modeStr = 'UNSPECIFIED';
        if (mode === EXACTLY) modeStr = 'EXACTLY';
        else if (mode === AT_MOST) modeStr = 'AT_MOST';
        return `MeasureSpec: ${modeStr} ${size}px`;
    }
}

// -----------------------------------------------------------------------------
// 2. LayoutParams
// -----------------------------------------------------------------------------

export const MATCH_PARENT = -1;
export const WRAP_CONTENT = -2;

export class LayoutParams {
    static MATCH_PARENT = MATCH_PARENT;
    static WRAP_CONTENT = WRAP_CONTENT;

    constructor(width = WRAP_CONTENT, height = WRAP_CONTENT, weight = 0) {
        this.width = width;
        this.height = height;
        // Margins: [left, top, right, bottom]
        this.margins = [0, 0, 0, 0];
        this.weight = Number(weight) || 0;
        this.gravity = 0;
        // Rules for RelativeLayout (verb -> targetId/true)
        this.alignRules = {};
        // Rules for ConstraintLayout (constraintName -> targetId/string)
        this.constraints = {};
        // Additional custom layout parameters
        this.custom = {};
    }

    get marginLeft() { return this.margins[0]; }
    set marginLeft(val) { this.margins[0] = Math.max(0, val | 0); }
    get marginTop() { return this.margins[1]; }
    set marginTop(val) { this.margins[1] = Math.max(0, val | 0); }
    get marginRight() { return this.margins[2]; }
    set marginRight(val) { this.margins[2] = Math.max(0, val | 0); }
    get marginBottom() { return this.margins[3]; }
    set marginBottom(val) { this.margins[3] = Math.max(0, val | 0); }

    get marginStart() { return this.margins[0]; }
    set marginStart(val) { this.margins[0] = Math.max(0, val | 0); }
    get marginEnd() { return this.margins[2]; }
    set marginEnd(val) { this.margins[2] = Math.max(0, val | 0); }

    setMargins(left, top, right, bottom) {
        this.margins = [
            Math.max(0, left | 0),
            Math.max(0, top | 0),
            Math.max(0, right | 0),
            Math.max(0, bottom | 0)
        ];
    }
}

// -----------------------------------------------------------------------------
// 3. View Base Class
// -----------------------------------------------------------------------------

export const VISIBLE = 0;
export const INVISIBLE = 4;
export const GONE = 8;

export class View {
    static VISIBLE = VISIBLE;
    static INVISIBLE = INVISIBLE;
    static GONE = GONE;

    constructor(layoutParams = new LayoutParams()) {
        this.id = 0;
        this.tag = null;
        this.visibility = VISIBLE;
        this.layoutParams = layoutParams;
        // Padding: [left, top, right, bottom]
        this.padding = [0, 0, 0, 0];
        this.background = null;
        this.backgroundColor = null;
        this.backgroundTint = null;
        this.elevation = 0;
        this.alpha = 1.0;

        // Geometry bounds (relative to parent)
        this.left = 0;
        this.top = 0;
        this.right = 0;
        this.bottom = 0;

        // Measured dimensions
        this.measuredWidth = 0;
        this.measuredHeight = 0;

        // Parent hierarchy
        this.parent = null;

        // Interaction state
        this.isClickable = false;
        this.isFocusable = false;
        this.isPressed = false;
        this.isFocused = false;
        this.isSelected = false;

        // Event listeners
        this.onClickListener = null;
        this.onTouchListener = null;

        // Suggested minimums
        this.minWidth = 0;
        this.minHeight = 0;

        // Scroll state
        this.scrollX = 0;
        this.scrollY = 0;

        // Custom attributes map
        this.attributes = new Map();
    }

    get paddingLeft() { return this.padding[0]; }
    set paddingLeft(v) { this.padding[0] = Math.max(0, v | 0); }
    get paddingTop() { return this.padding[1]; }
    set paddingTop(v) { this.padding[1] = Math.max(0, v | 0); }
    get paddingRight() { return this.padding[2]; }
    set paddingRight(v) { this.padding[2] = Math.max(0, v | 0); }
    get paddingBottom() { return this.padding[3]; }
    set paddingBottom(v) { this.padding[3] = Math.max(0, v | 0); }

    get paddingStart() { return this.padding[0]; }
    set paddingStart(v) { this.padding[0] = Math.max(0, v | 0); }
    get paddingEnd() { return this.padding[2]; }
    set paddingEnd(v) { this.padding[2] = Math.max(0, v | 0); }

    setPadding(left, top, right, bottom) {
        this.padding = [
            Math.max(0, left | 0),
            Math.max(0, top | 0),
            Math.max(0, right | 0),
            Math.max(0, bottom | 0)
        ];
    }

    getWidth() { return Math.max(0, this.right - this.left); }
    getHeight() { return Math.max(0, this.bottom - this.top); }
    getMeasuredWidth() { return this.measuredWidth; }
    getMeasuredHeight() { return this.measuredHeight; }

    setMeasuredDimension(measuredWidth, measuredHeight) {
        this.measuredWidth = Math.max(0, Math.round(measuredWidth));
        this.measuredHeight = Math.max(0, Math.round(measuredHeight));
    }

    /**
     * Main measure pass entrypoint.
     */
    measure(widthMeasureSpec, heightMeasureSpec) {
        if (this.visibility === GONE) {
            this.setMeasuredDimension(0, 0);
            return;
        }
        this.onMeasure(widthMeasureSpec, heightMeasureSpec);
    }

    /**
     * Default measurement implementation.
     */
    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        let minW = this.minWidth + this.paddingLeft + this.paddingRight;
        let minH = this.minHeight + this.paddingTop + this.paddingBottom;

        if (this.layoutParams) {
            if (this.layoutParams.width >= 0) minW = Math.max(minW, this.layoutParams.width);
            if (this.layoutParams.height >= 0) minH = Math.max(minH, this.layoutParams.height);
        }

        let w = View.getDefaultSize(minW, widthMeasureSpec);
        let h = View.getDefaultSize(minH, heightMeasureSpec);

        if (this.layoutParams) {
            if (MeasureSpec.getMode(widthMeasureSpec) === UNSPECIFIED && this.layoutParams.width >= 0) {
                w = this.layoutParams.width;
            }
            if (MeasureSpec.getMode(heightMeasureSpec) === UNSPECIFIED && this.layoutParams.height >= 0) {
                h = this.layoutParams.height;
            }
        }
        this.setMeasuredDimension(w, h);
    }

    static getDefaultSize(size, measureSpec) {
        let result = size;
        const specMode = MeasureSpec.getMode(measureSpec);
        const specSize = MeasureSpec.getSize(measureSpec);

        switch (specMode) {
            case UNSPECIFIED:
                result = size;
                break;
            case AT_MOST:
            case EXACTLY:
                result = specSize;
                break;
        }
        return result;
    }

    static resolveSize(size, measureSpec) {
        const specMode = MeasureSpec.getMode(measureSpec);
        const specSize = MeasureSpec.getSize(measureSpec);
        switch (specMode) {
            case EXACTLY:
                return specSize;
            case AT_MOST:
                return Math.min(size, specSize);
            case UNSPECIFIED:
            default:
                return size;
        }
    }

    /**
     * Main layout pass entrypoint.
     */
    layout(l, t, r, b) {
        const changed = (this.left !== l || this.top !== t || this.right !== r || this.bottom !== b);
        this.left = l;
        this.top = t;
        this.right = r;
        this.bottom = b;
        this.onLayout(changed, l, t, r, b);
    }

    onLayout(changed, l, t, r, b) {
        // Base view has no children
    }

    /**
     * Renders this view onto a 2D canvas context.
     * @param {CanvasRenderingContext2D|OffscreenCanvasRenderingContext2D} ctx
     */
    draw(ctx) {
        if (this.visibility !== VISIBLE || this.alpha <= 0) return;

        const w = this.getWidth();
        const h = this.getHeight();
        if (w <= 0 || h <= 0) return;

        ctx.save?.();
        if (this.alpha < 1.0) ctx.globalAlpha *= this.alpha;

        // Draw background
        this.drawBackground(ctx, w, h);

        // Draw view contents
        this.onDraw(ctx);

        ctx.restore?.();
    }

    drawBackground(ctx, w, h) {
        const bg = this.backgroundColor || this.background;
        if (bg && typeof bg === 'string') {
            ctx.fillStyle = bg;
            ctx.fillRect(this.left, this.top, w, h);
        } else if (typeof bg === 'function') {
            bg(ctx, this.left, this.top, w, h);
        }
    }

    onDraw(ctx) {
        // Subclasses override
    }

    setOnClickListener(listener) {
        this.isClickable = true;
        this.onClickListener = listener;
    }

    setOnTouchListener(listener) {
        this.onTouchListener = listener;
    }

    /**
     * Dispatches touch events to listeners and onTouchEvent.
     * @param {object} event - MotionEvent { action, x, y }
     * @returns {boolean} True if consumed.
     */
    dispatchTouchEvent(event) {
        if (this.visibility !== VISIBLE) return false;

        if (this.onTouchListener && typeof this.onTouchListener === 'function') {
            if (this.onTouchListener(this, event)) return true;
        }
        return this.onTouchEvent(event);
    }

    /**
     * Handles touch interactions and click invocation.
     */
    onTouchEvent(event) {
        if (!this.isClickable && !this.isFocusable) return false;

        const action = event.action;
        if (action === 0 /* ACTION_DOWN */) {
            this.isPressed = true;
            return true;
        } else if (action === 1 /* ACTION_UP */) {
            const wasPressed = this.isPressed;
            this.isPressed = false;
            if (wasPressed && this.onClickListener) {
                this.onClickListener(this);
                return true;
            }
            return true;
        } else if (action === 3 /* ACTION_CANCEL */) {
            this.isPressed = false;
            return true;
        }
        return false;
    }

    getChildCount() {
        return 0;
    }

    getChildAt(index) {
        return null;
    }

    findViewById(id) {
        if (this.id === id && id !== 0) return this;
        return null;
    }

    getRootView() {
        let cur = this;
        while (cur.parent) cur = cur.parent;
        return cur;
    }

    getLocationOnScreen() {
        let x = this.left;
        let y = this.top;
        let p = this.parent;
        while (p) {
            x += p.left - (p.scrollX || 0);
            y += p.top - (p.scrollY || 0);
            p = p.parent;
        }
        return { x, y };
    }
}

// -----------------------------------------------------------------------------
// 4. ViewGroup Container Base Class
// -----------------------------------------------------------------------------

export class ViewGroup extends View {
    constructor(layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT)) {
        super(layoutParams);
        this.children = [];
    }

    addView(child, indexOrParams = -1, params = null) {
        if (!child || !(child instanceof View)) return;

        let index = -1;
        let layoutParams = null;

        if (typeof indexOrParams === 'number') {
            index = indexOrParams;
            layoutParams = params;
        } else if (indexOrParams instanceof LayoutParams) {
            layoutParams = indexOrParams;
        }

        if (layoutParams) child.layoutParams = layoutParams;
        if (!child.layoutParams) child.layoutParams = new LayoutParams();

        child.parent = this;

        if (index >= 0 && index < this.children.length) {
            this.children.splice(index, 0, child);
        } else {
            this.children.push(child);
        }
    }

    removeView(child) {
        const idx = this.children.indexOf(child);
        if (idx >= 0) {
            this.children.splice(idx, 1);
            child.parent = null;
        }
    }

    removeViewAt(index) {
        if (index >= 0 && index < this.children.length) {
            const child = this.children.splice(index, 1)[0];
            child.parent = null;
        }
    }

    removeAllViews() {
        for (const child of this.children) child.parent = null;
        this.children = [];
    }

    getChildCount() {
        return this.children.length;
    }

    getChildAt(index) {
        return this.children[index] || null;
    }

    findViewById(id) {
        if (this.id === id && id !== 0) return this;
        for (const child of this.children) {
            const found = child.findViewById(id);
            if (found) return found;
        }
        return null;
    }

    /**
     * Measures all non-GONE children.
     */
    measureChildren(widthMeasureSpec, heightMeasureSpec) {
        for (const child of this.children) {
            if (child.visibility !== GONE) {
                this.measureChild(child, widthMeasureSpec, heightMeasureSpec);
            }
        }
    }

    measureChild(child, parentWidthMeasureSpec, parentHeightMeasureSpec) {
        const lp = child.layoutParams || new LayoutParams();
        const childWSpec = ViewGroup.getChildMeasureSpec(
            parentWidthMeasureSpec,
            this.paddingLeft + this.paddingRight,
            lp.width
        );
        const childHSpec = ViewGroup.getChildMeasureSpec(
            parentHeightMeasureSpec,
            this.paddingTop + this.paddingBottom,
            lp.height
        );
        child.measure(childWSpec, childHSpec);
    }

    measureChildWithMargins(child, parentWidthMeasureSpec, widthUsed, parentHeightMeasureSpec, heightUsed) {
        const lp = child.layoutParams || new LayoutParams();
        const childWSpec = ViewGroup.getChildMeasureSpec(
            parentWidthMeasureSpec,
            this.paddingLeft + this.paddingRight + lp.marginLeft + lp.marginRight + widthUsed,
            lp.width
        );
        const childHSpec = ViewGroup.getChildMeasureSpec(
            parentHeightMeasureSpec,
            this.paddingTop + this.paddingBottom + lp.marginTop + lp.marginBottom + heightUsed,
            lp.height
        );
        child.measure(childWSpec, childHSpec);
    }

    /**
     * Computes MeasureSpec for a child based on parent's spec and child layout dimension.
     */
    static getChildMeasureSpec(spec, padding, childDimension) {
        const specMode = MeasureSpec.getMode(spec);
        const specSize = MeasureSpec.getSize(spec);
        const size = Math.max(0, specSize - padding);

        let resultSize = 0;
        let resultMode = UNSPECIFIED;

        switch (specMode) {
            case EXACTLY:
                if (childDimension > 0) {
                    resultSize = childDimension;
                    resultMode = EXACTLY;
                } else if (childDimension === 0) {
                    // MATCH_CONSTRAINT (0dp): flexible under parent constraint
                    resultSize = size;
                    resultMode = AT_MOST;
                } else if (childDimension === MATCH_PARENT) {
                    resultSize = size;
                    resultMode = EXACTLY;
                } else if (childDimension === WRAP_CONTENT) {
                    resultSize = size;
                    resultMode = AT_MOST;
                }
                break;

            case AT_MOST:
                if (childDimension > 0) {
                    resultSize = childDimension;
                    resultMode = EXACTLY;
                } else if (childDimension === 0) {
                    // MATCH_CONSTRAINT (0dp): flexible under parent constraint
                    resultSize = size;
                    resultMode = AT_MOST;
                } else if (childDimension === MATCH_PARENT) {
                    resultSize = size;
                    resultMode = AT_MOST;
                } else if (childDimension === WRAP_CONTENT) {
                    resultSize = size;
                    resultMode = AT_MOST;
                }
                break;

            case UNSPECIFIED:
                if (childDimension > 0) {
                    resultSize = childDimension;
                    resultMode = EXACTLY;
                } else if (childDimension === 0) {
                    resultSize = 0;
                    resultMode = UNSPECIFIED;
                } else if (childDimension === MATCH_PARENT) {
                    resultSize = 0;
                    resultMode = UNSPECIFIED;
                } else if (childDimension === WRAP_CONTENT) {
                    resultSize = 0;
                    resultMode = UNSPECIFIED;
                }
                break;
        }
        return MeasureSpec.makeMeasureSpec(resultSize, resultMode);
    }

    onLayout(changed, l, t, r, b) {
        // Base ViewGroup default layout
        const w = r - l;
        const h = b - t;
        for (const child of this.children) {
            if (child.visibility !== GONE) {
                child.layout(this.paddingLeft, this.paddingTop, w - this.paddingRight, h - this.paddingBottom);
            }
        }
    }

    draw(ctx) {
        if (this.visibility !== VISIBLE || this.alpha <= 0) return;

        const w = this.getWidth();
        const h = this.getHeight();
        if (w <= 0 || h <= 0) return;

        ctx.save?.();
        if (this.alpha < 1.0) ctx.globalAlpha *= this.alpha;

        // Draw container background
        this.drawBackground(ctx, w, h);

        // Draw container custom contents
        this.onDraw(ctx);

        // Draw children with container clipping & scroll offset translation
        this.dispatchDraw(ctx);

        ctx.restore?.();
    }

    dispatchDraw(ctx) {
        if (!ctx) return;
        ctx.save?.();
        // Clip to container bounds
        if (ctx.beginPath && ctx.rect && ctx.clip) {
            ctx.beginPath();
            ctx.rect(this.left, this.top, this.getWidth(), this.getHeight());
            ctx.clip();
        }

        // Apply container scroll translation if any
        if ((this.scrollX || this.scrollY) && ctx.translate) {
            ctx.translate(-(this.scrollX || 0), -(this.scrollY || 0));
        }

        // Draw children
        for (const child of this.children) {
            if (child.visibility === VISIBLE) {
                child.draw(ctx);
            }
        }
        ctx.restore?.();
    }

    /**
     * Top-down reverse-Z hit testing for touch events.
     */
    dispatchTouchEvent(event) {
        if (this.visibility !== VISIBLE) return false;

        const x = event.x;
        const y = event.y;

        // Test children in reverse order (topmost child first)
        for (let i = this.children.length - 1; i >= 0; i--) {
            const child = this.children[i];
            if (child.visibility !== VISIBLE) continue;

            const childLeft = child.left - this.scrollX;
            const childTop = child.top - this.scrollY;
            const childRight = child.right - this.scrollX;
            const childBottom = child.bottom - this.scrollY;

            if (x >= childLeft && x <= childRight && y >= childTop && y <= childBottom) {
                const transformedEvent = {
                    ...event,
                    x: x - childLeft,
                    y: y - childTop
                };
                if (child.dispatchTouchEvent(transformedEvent)) {
                    return true;
                }
            }
        }

        // Fall back to this container's own touch handling
        return super.dispatchTouchEvent(event);
    }
}

// -----------------------------------------------------------------------------
// 5. Layout Containers
// -----------------------------------------------------------------------------

/**
 * FrameLayout: Stacks children on top of each other with gravity support.
 */
export class FrameLayout extends ViewGroup {
    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        let maxWidth = 0;
        let maxHeight = 0;

        for (const child of this.children) {
            if (child.visibility !== GONE) {
                this.measureChildWithMargins(child, widthMeasureSpec, 0, heightMeasureSpec, 0);
                const lp = child.layoutParams;
                maxWidth = Math.max(maxWidth, child.measuredWidth + lp.marginLeft + lp.marginRight);
                maxHeight = Math.max(maxHeight, child.measuredHeight + lp.marginTop + lp.marginBottom);
            }
        }

        maxWidth += this.paddingLeft + this.paddingRight;
        maxHeight += this.paddingTop + this.paddingBottom;

        maxWidth = Math.max(maxWidth, this.minWidth);
        maxHeight = Math.max(maxHeight, this.minHeight);

        const measuredW = View.resolveSize(maxWidth, widthMeasureSpec);
        const measuredH = View.resolveSize(maxHeight, heightMeasureSpec);
        this.setMeasuredDimension(measuredW, measuredH);
    }

    onLayout(changed, l, t, r, b) {
        const parentLeft = this.paddingLeft;
        const parentRight = this.getWidth() - this.paddingRight;
        const parentTop = this.paddingTop;
        const parentBottom = this.getHeight() - this.paddingBottom;

        const parentWidth = parentRight - parentLeft;
        const parentHeight = parentBottom - parentTop;

        for (const child of this.children) {
            if (child.visibility === GONE) continue;

            const lp = child.layoutParams;
            const w = child.measuredWidth;
            const h = child.measuredHeight;

            let childLeft = parentLeft + lp.marginLeft;
            let childTop = parentTop + lp.marginTop;

            // Gravity positioning
            const gravity = lp.gravity || 0;
            const hGravity = gravity & 0x07; // LEFT=3, RIGHT=5, CENTER_HORIZONTAL=1
            const vGravity = gravity & 0x70; // TOP=0x30, BOTTOM=0x50, CENTER_VERTICAL=0x10

            if (hGravity === 1 || gravity === 17 /* CENTER */) {
                childLeft = parentLeft + Math.round((parentWidth - w) / 2) + lp.marginLeft - lp.marginRight;
            } else if (hGravity === 5 /* RIGHT */) {
                childLeft = parentRight - w - lp.marginRight;
            }

            if (vGravity === 0x10 || gravity === 17 /* CENTER */) {
                childTop = parentTop + Math.round((parentHeight - h) / 2) + lp.marginTop - lp.marginBottom;
            } else if (vGravity === 0x50 /* BOTTOM */) {
                childTop = parentBottom - h - lp.marginBottom;
            }

            child.layout(childLeft, childTop, childLeft + w, childTop + h);
        }
    }
}

export const HORIZONTAL = 0;
export const VERTICAL = 1;

/**
 * LinearLayout: Arranges children horizontally or vertically with layout_weight support.
 */
export class LinearLayout extends ViewGroup {
    static HORIZONTAL = HORIZONTAL;
    static VERTICAL = VERTICAL;

    constructor(orientation = VERTICAL, layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT)) {
        super(layoutParams);
        this.orientation = orientation;
        this.gravity = 0;
        this.weightSum = 0;
    }

    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        if (this.orientation === VERTICAL) {
            this.measureVertical(widthMeasureSpec, heightMeasureSpec);
        } else {
            this.measureHorizontal(widthMeasureSpec, heightMeasureSpec);
        }
    }

    measureVertical(widthMeasureSpec, heightMeasureSpec) {
        let totalWeight = 0;
        let usedHeight = 0;
        let maxWidth = 0;

        for (const child of this.children) {
            if (child.visibility === GONE) continue;

            const lp = child.layoutParams;
            totalWeight += lp.weight || 0;

            if ((lp.weight || 0) === 0 || lp.height !== 0) {
                this.measureChildWithMargins(child, widthMeasureSpec, 0, heightMeasureSpec, usedHeight);
                usedHeight += child.measuredHeight + lp.marginTop + lp.marginBottom;
                maxWidth = Math.max(maxWidth, child.measuredWidth + lp.marginLeft + lp.marginRight);
            } else {
                usedHeight += lp.marginTop + lp.marginBottom;
            }
        }

        usedHeight += this.paddingTop + this.paddingBottom;
        maxWidth += this.paddingLeft + this.paddingRight;
        maxWidth = Math.max(maxWidth, this.minWidth);

        const targetHeight = View.getDefaultSize(usedHeight, heightMeasureSpec);
        const remainingSpace = targetHeight - usedHeight;

        // Weight distribution pass
        if (totalWeight > 0 && remainingSpace !== 0) {
            const weightRatio = this.weightSum > 0 ? this.weightSum : totalWeight;
            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const lp = child.layoutParams;
                if ((lp.weight || 0) > 0) {
                    const extra = Math.round((lp.weight / weightRatio) * remainingSpace);
                    const childH = Math.max(0, (lp.height === 0 ? 0 : child.measuredHeight) + extra);
                    const childHSpec = MeasureSpec.makeMeasureSpec(childH, MeasureSpec.EXACTLY);
                    const childWSpec = ViewGroup.getChildMeasureSpec(
                        widthMeasureSpec,
                        this.paddingLeft + this.paddingRight + lp.marginLeft + lp.marginRight,
                        lp.width
                    );
                    child.measure(childWSpec, childHSpec);
                    maxWidth = Math.max(maxWidth, child.measuredWidth + lp.marginLeft + lp.marginRight + this.paddingLeft + this.paddingRight);
                }
            }
        }

        const measuredW = View.getDefaultSize(maxWidth, widthMeasureSpec);
        this.setMeasuredDimension(measuredW, targetHeight);
    }

    measureHorizontal(widthMeasureSpec, heightMeasureSpec) {
        let totalWeight = 0;
        let usedWidth = 0;
        let maxHeight = 0;

        for (const child of this.children) {
            if (child.visibility === GONE) continue;

            const lp = child.layoutParams;
            totalWeight += lp.weight || 0;

            if ((lp.weight || 0) === 0 || lp.width !== 0) {
                this.measureChildWithMargins(child, widthMeasureSpec, usedWidth, heightMeasureSpec, 0);
                usedWidth += child.measuredWidth + lp.marginLeft + lp.marginRight;
                maxHeight = Math.max(maxHeight, child.measuredHeight + lp.marginTop + lp.marginBottom);
            } else {
                usedWidth += lp.marginLeft + lp.marginRight;
            }
        }

        usedWidth += this.paddingLeft + this.paddingRight;
        maxHeight += this.paddingTop + this.paddingBottom;
        maxHeight = Math.max(maxHeight, this.minHeight);

        const targetWidth = View.getDefaultSize(usedWidth, widthMeasureSpec);
        const remainingSpace = targetWidth - usedWidth;

        // Weight distribution pass
        if (totalWeight > 0 && remainingSpace !== 0) {
            const weightRatio = this.weightSum > 0 ? this.weightSum : totalWeight;
            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const lp = child.layoutParams;
                if ((lp.weight || 0) > 0) {
                    const extra = Math.round((lp.weight / weightRatio) * remainingSpace);
                    const childW = Math.max(0, (lp.width === 0 ? 0 : child.measuredWidth) + extra);
                    const childWSpec = MeasureSpec.makeMeasureSpec(childW, MeasureSpec.EXACTLY);
                    const childHSpec = ViewGroup.getChildMeasureSpec(
                        heightMeasureSpec,
                        this.paddingTop + this.paddingBottom + lp.marginTop + lp.marginBottom,
                        lp.height
                    );
                    child.measure(childWSpec, childHSpec);
                    maxHeight = Math.max(maxHeight, child.measuredHeight + lp.marginTop + lp.marginBottom + this.paddingTop + this.paddingBottom);
                }
            }
        }

        const measuredH = View.getDefaultSize(maxHeight, heightMeasureSpec);
        this.setMeasuredDimension(targetWidth, measuredH);
    }

    onLayout(changed, l, t, r, b) {
        if (this.orientation === VERTICAL) {
            let curY = this.paddingTop;
            const parentLeft = this.paddingLeft;
            const parentRight = this.getWidth() - this.paddingRight;
            const parentWidth = parentRight - parentLeft;

            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const lp = child.layoutParams;
                const w = child.measuredWidth;
                const h = child.measuredHeight;

                curY += lp.marginTop;
                let childLeft = parentLeft + lp.marginLeft;

                const gravity = lp.gravity || this.gravity || 0;
                const hGravity = gravity & 0x07;
                if (hGravity === 1 || gravity === 17 /* CENTER */) {
                    childLeft = parentLeft + Math.round((parentWidth - w) / 2) + lp.marginLeft - lp.marginRight;
                } else if (hGravity === 5 /* RIGHT */) {
                    childLeft = parentRight - w - lp.marginRight;
                }

                child.layout(childLeft, curY, childLeft + w, curY + h);
                curY += h + lp.marginBottom;
            }
        } else {
            let curX = this.paddingLeft;
            const parentTop = this.paddingTop;
            const parentBottom = this.getHeight() - this.paddingBottom;
            const parentHeight = parentBottom - parentTop;

            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const lp = child.layoutParams;
                const w = child.measuredWidth;
                const h = child.measuredHeight;

                curX += lp.marginLeft;
                let childTop = parentTop + lp.marginTop;

                const gravity = lp.gravity || this.gravity || 0;
                const vGravity = gravity & 0x70;
                if (vGravity === 0x10 || gravity === 17 /* CENTER */) {
                    childTop = parentTop + Math.round((parentHeight - h) / 2) + lp.marginTop - lp.marginBottom;
                } else if (vGravity === 0x50 /* BOTTOM */) {
                    childTop = parentBottom - h - lp.marginBottom;
                }

                child.layout(curX, childTop, curX + w, childTop + h);
                curX += w + lp.marginRight;
            }
        }
    }
}

/**
 * RelativeLayout: Resolves spatial rules and anchor dependencies between children.
 */
export class RelativeLayout extends ViewGroup {
    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        // Measure all children with unconstrained AT_MOST
        this.measureChildren(widthMeasureSpec, heightMeasureSpec);

        let maxRight = 0;
        let maxBottom = 0;
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            maxRight = Math.max(maxRight, child.measuredWidth + lp.marginLeft + lp.marginRight);
            maxBottom = Math.max(maxBottom, child.measuredHeight + lp.marginTop + lp.marginBottom);
        }

        maxRight += this.paddingLeft + this.paddingRight;
        maxBottom += this.paddingTop + this.paddingBottom;

        const w = View.getDefaultSize(Math.max(maxRight, this.minWidth), widthMeasureSpec);
        const h = View.getDefaultSize(Math.max(maxBottom, this.minHeight), heightMeasureSpec);
        this.setMeasuredDimension(w, h);
    }

    onLayout(changed, l, t, r, b) {
        const parentLeft = this.paddingLeft;
        const parentRight = this.getWidth() - this.paddingRight;
        const parentTop = this.paddingTop;
        const parentBottom = this.getHeight() - this.paddingBottom;
        const parentWidth = parentRight - parentLeft;
        const parentHeight = parentBottom - parentTop;

        const childBounds = new Map(); // child -> { left, top, right, bottom }

        // Initial pass
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            const w = child.measuredWidth;
            const h = child.measuredHeight;

            let cl = parentLeft + lp.marginLeft;
            let ct = parentTop + lp.marginTop;
            let cr = cl + w;
            let cb = ct + h;

            // Parent alignment rules
            const rules = lp.alignRules || {};
            if (rules.alignParentTop || rules.layout_alignParentTop) {
                ct = parentTop + lp.marginTop;
                cb = ct + h;
            }
            if (rules.alignParentBottom || rules.layout_alignParentBottom) {
                cb = parentBottom - lp.marginBottom;
                ct = cb - h;
            }
            if (rules.alignParentLeft || rules.layout_alignParentLeft || rules.alignParentStart || rules.layout_alignParentStart) {
                cl = parentLeft + lp.marginLeft;
                cr = cl + w;
            }
            if (rules.alignParentRight || rules.layout_alignParentRight || rules.alignParentEnd || rules.layout_alignParentEnd) {
                cr = parentRight - lp.marginRight;
                cl = cr - w;
            }
            if (rules.centerInParent || rules.layout_centerInParent) {
                cl = parentLeft + Math.round((parentWidth - w) / 2) + lp.marginLeft - lp.marginRight;
                cr = cl + w;
                ct = parentTop + Math.round((parentHeight - h) / 2) + lp.marginTop - lp.marginBottom;
                cb = ct + h;
            } else {
                if (rules.centerHorizontal || rules.layout_centerHorizontal) {
                    cl = parentLeft + Math.round((parentWidth - w) / 2) + lp.marginLeft - lp.marginRight;
                    cr = cl + w;
                }
                if (rules.centerVertical || rules.layout_centerVertical) {
                    ct = parentTop + Math.round((parentHeight - h) / 2) + lp.marginTop - lp.marginBottom;
                    cb = ct + h;
                }
            }

            childBounds.set(child, { cl, ct, cr, cb });
        }

        // Anchor dependency resolution passes
        for (let pass = 0; pass < 3; pass++) {
            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const lp = child.layoutParams;
                const rules = lp.alignRules || {};
                const bounds = childBounds.get(child);
                if (!bounds) continue;
                const w = child.measuredWidth;
                const h = child.measuredHeight;

                let topAnchor = null;
                let bottomAnchor = null;
                let leftAnchor = null;
                let rightAnchor = null;

                // Vertical anchors
                if (rules.alignParentTop || rules.layout_alignParentTop) {
                    topAnchor = parentTop + lp.marginTop;
                }
                if (rules.alignParentBottom || rules.layout_alignParentBottom) {
                    bottomAnchor = parentBottom - lp.marginBottom;
                }

                const belowId = rules.below || rules.layout_below;
                if (belowId) {
                    const target = this.findViewById(belowId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) topAnchor = tb.cb + lp.marginTop;
                }

                const aboveId = rules.above || rules.layout_above;
                if (aboveId) {
                    const target = this.findViewById(aboveId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) bottomAnchor = tb.ct - lp.marginBottom;
                }

                const alignTopId = rules.alignTop || rules.layout_alignTop;
                if (alignTopId) {
                    const target = this.findViewById(alignTopId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) topAnchor = tb.ct + lp.marginTop;
                }

                const alignBottomId = rules.alignBottom || rules.layout_alignBottom;
                if (alignBottomId) {
                    const target = this.findViewById(alignBottomId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) bottomAnchor = tb.cb - lp.marginBottom;
                }

                // Horizontal anchors
                if (rules.alignParentLeft || rules.layout_alignParentLeft || rules.alignParentStart || rules.layout_alignParentStart) {
                    leftAnchor = parentLeft + lp.marginLeft;
                }
                if (rules.alignParentRight || rules.layout_alignParentRight || rules.alignParentEnd || rules.layout_alignParentEnd) {
                    rightAnchor = parentRight - lp.marginRight;
                }

                const toRightId = rules.toRightOf || rules.layout_toRightOf || rules.toEndOf || rules.layout_toEndOf;
                if (toRightId) {
                    const target = this.findViewById(toRightId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) leftAnchor = tb.cr + lp.marginLeft;
                }

                const toLeftId = rules.toLeftOf || rules.layout_toLeftOf || rules.toStartOf || rules.layout_toStartOf;
                if (toLeftId) {
                    const target = this.findViewById(toLeftId);
                    const tb = target ? childBounds.get(target) : null;
                    if (tb) rightAnchor = tb.cl - lp.marginRight;
                }

                // Apply vertical anchors
                if (topAnchor !== null && bottomAnchor !== null) {
                    bounds.ct = topAnchor;
                    bounds.cb = bottomAnchor;
                    if (lp.height >= 0 && lp.height !== MATCH_PARENT) {
                        bounds.cb = bounds.ct + h;
                    }
                } else if (topAnchor !== null) {
                    bounds.ct = topAnchor;
                    bounds.cb = topAnchor + h;
                } else if (bottomAnchor !== null) {
                    bounds.cb = bottomAnchor;
                    bounds.ct = bottomAnchor - h;
                }

                // Apply horizontal anchors
                if (leftAnchor !== null && rightAnchor !== null) {
                    bounds.cl = leftAnchor;
                    bounds.cr = rightAnchor;
                    if (lp.width >= 0 && lp.width !== MATCH_PARENT) {
                        bounds.cr = bounds.cl + w;
                    }
                } else if (leftAnchor !== null) {
                    bounds.cl = leftAnchor;
                    bounds.cr = leftAnchor + w;
                } else if (rightAnchor !== null) {
                    bounds.cr = rightAnchor;
                    bounds.cl = rightAnchor - w;
                }
            }
        }

        // Apply resolved bounds
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const bnd = childBounds.get(child);
            if (bnd) {
                child.layout(bnd.cl, bnd.ct, bnd.cr, bnd.cb);
            }
        }
    }
}

/**
 * ConstraintLayout: Constraint graph anchor solver.
 */
export class ConstraintLayout extends ViewGroup {
    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        const parentWSize = MeasureSpec.getSize(widthMeasureSpec);
        const parentHSize = MeasureSpec.getSize(heightMeasureSpec);
        const parentWMode = MeasureSpec.getMode(widthMeasureSpec);
        const parentHMode = MeasureSpec.getMode(heightMeasureSpec);

        const parentLeft = this.paddingLeft;
        const parentRight = (parentWMode !== UNSPECIFIED ? parentWSize : 100000) - this.paddingRight;
        const parentTop = this.paddingTop;
        const parentBottom = (parentHMode !== UNSPECIFIED ? parentHSize : 100000) - this.paddingBottom;

        // Pass 1: Measure fixed and wrap_content children, initial measure for 0dp children
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            this.measureChildWithMargins(child, widthMeasureSpec, 0, heightMeasureSpec, 0);
        }

        // Internal relaxation pass to estimate spans for MATCH_CONSTRAINT (0dp) children
        const estBounds = new Map();
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            const w = child.measuredWidth;
            const h = child.measuredHeight;
            let cl = parentLeft + lp.marginLeft;
            let ct = parentTop + lp.marginTop;
            let cr = cl + w;
            let cb = ct + h;
            estBounds.set(child, { cl, ct, cr, cb, w, h, lp });
        }

        for (let pass = 0; pass < 3; pass++) {
            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const b = estBounds.get(child);
                const lp = child.layoutParams;
                const c = lp.constraints || {};

                let leftAnchor = null;
                let rightAnchor = null;
                let topAnchor = null;
                let bottomAnchor = null;

                const startToStart = c.layout_constraintStart_toStartOf ?? c.layout_constraintLeft_toLeftOf;
                if (startToStart === 'parent' || startToStart === 0 || startToStart === -1) {
                    leftAnchor = parentLeft;
                } else if (startToStart !== undefined && startToStart !== null) {
                    const t = this.findViewById(startToStart);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) leftAnchor = tb.cl;
                }

                const startToEnd = c.layout_constraintStart_toEndOf ?? c.layout_constraintLeft_toRightOf;
                if (startToEnd !== undefined && startToEnd !== null) {
                    const t = this.findViewById(startToEnd);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) leftAnchor = tb.cr;
                }

                const endToEnd = c.layout_constraintEnd_toEndOf ?? c.layout_constraintRight_toRightOf;
                if (endToEnd === 'parent' || endToEnd === 0 || endToEnd === -1) {
                    rightAnchor = parentRight;
                } else if (endToEnd !== undefined && endToEnd !== null) {
                    const t = this.findViewById(endToEnd);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) rightAnchor = tb.cr;
                }

                const endToStart = c.layout_constraintEnd_toStartOf ?? c.layout_constraintRight_toLeftOf;
                if (endToStart !== undefined && endToStart !== null) {
                    const t = this.findViewById(endToStart);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) rightAnchor = tb.cl;
                }

                const topToTop = c.layout_constraintTop_toTopOf;
                if (topToTop === 'parent' || topToTop === 0 || topToTop === -1) {
                    topAnchor = parentTop;
                } else if (topToTop !== undefined && topToTop !== null) {
                    const t = this.findViewById(topToTop);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) topAnchor = tb.ct;
                }

                const topToBottom = c.layout_constraintTop_toBottomOf;
                if (topToBottom !== undefined && topToBottom !== null) {
                    const t = this.findViewById(topToBottom);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) topAnchor = tb.cb;
                }

                const bottomToBottom = c.layout_constraintBottom_toBottomOf;
                if (bottomToBottom === 'parent' || bottomToBottom === 0 || bottomToBottom === -1) {
                    bottomAnchor = parentBottom;
                } else if (bottomToBottom !== undefined && bottomToBottom !== null) {
                    const t = this.findViewById(bottomToBottom);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) bottomAnchor = tb.cb;
                }

                const bottomToTop = c.layout_constraintBottom_toTopOf;
                if (bottomToTop !== undefined && bottomToTop !== null) {
                    const t = this.findViewById(bottomToTop);
                    const tb = t ? estBounds.get(t) : null;
                    if (tb) bottomAnchor = tb.ct;
                }

                if (leftAnchor !== null && rightAnchor !== null) {
                    const availW = Math.max(0, rightAnchor - leftAnchor - lp.marginLeft - lp.marginRight);
                    if (lp.width === 0) {
                        b.cl = leftAnchor + lp.marginLeft;
                        b.cr = Math.max(b.cl, rightAnchor - lp.marginRight);
                        b.w = b.cr - b.cl;
                    } else {
                        const bias = (typeof c.layout_constraintHorizontal_bias === 'number' && !isNaN(c.layout_constraintHorizontal_bias)) ? c.layout_constraintHorizontal_bias : 0.5;
                        b.cl = leftAnchor + lp.marginLeft + Math.round((availW - b.w) * bias);
                        b.cr = b.cl + b.w;
                    }
                } else if (leftAnchor !== null) {
                    b.cl = leftAnchor + lp.marginLeft;
                    b.cr = b.cl + b.w;
                } else if (rightAnchor !== null) {
                    b.cr = rightAnchor - lp.marginRight;
                    b.cl = b.cr - b.w;
                }

                if (topAnchor !== null && bottomAnchor !== null) {
                    const availH = Math.max(0, bottomAnchor - topAnchor - lp.marginTop - lp.marginBottom);
                    if (lp.height === 0) {
                        b.ct = topAnchor + lp.marginTop;
                        b.cb = Math.max(b.ct, bottomAnchor - lp.marginBottom);
                        b.h = b.cb - b.ct;
                    } else {
                        const bias = (typeof c.layout_constraintVertical_bias === 'number' && !isNaN(c.layout_constraintVertical_bias)) ? c.layout_constraintVertical_bias : 0.5;
                        b.ct = topAnchor + lp.marginTop + Math.round((availH - b.h) * bias);
                        b.cb = b.ct + b.h;
                    }
                } else if (topAnchor !== null) {
                    b.ct = topAnchor + lp.marginTop;
                    b.cb = b.ct + b.h;
                } else if (bottomAnchor !== null) {
                    b.cb = bottomAnchor - lp.marginBottom;
                    b.ct = b.cb - b.h;
                }

                if (b.cr < b.cl) b.cr = b.cl;
                if (b.cb < b.ct) b.cb = b.ct;
            }
        }

        // Pass 2: Remeasure MATCH_CONSTRAINT (0dp) children with EXACTLY(resolvedSpan)
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            let remeasure = false;
            let childWSpec = null;
            let childHSpec = null;

            if (lp.width === 0) {
                const b = estBounds.get(child);
                const resolvedSpanW = b ? Math.max(0, b.cr - b.cl) : 0;
                childWSpec = MeasureSpec.makeMeasureSpec(resolvedSpanW, EXACTLY);
                remeasure = true;
            } else {
                childWSpec = ViewGroup.getChildMeasureSpec(
                    widthMeasureSpec,
                    this.paddingLeft + this.paddingRight + lp.marginLeft + lp.marginRight,
                    lp.width
                );
            }

            if (lp.height === 0) {
                const b = estBounds.get(child);
                const resolvedSpanH = b ? Math.max(0, b.cb - b.ct) : 0;
                childHSpec = MeasureSpec.makeMeasureSpec(resolvedSpanH, EXACTLY);
                remeasure = true;
            } else {
                childHSpec = ViewGroup.getChildMeasureSpec(
                    heightMeasureSpec,
                    this.paddingTop + this.paddingBottom + lp.marginTop + lp.marginBottom,
                    lp.height
                );
            }

            if (remeasure) {
                child.measure(childWSpec, childHSpec);
                const b = estBounds.get(child);
                if (b) {
                    b.w = child.measuredWidth;
                    b.h = child.measuredHeight;
                    b.cb = b.ct + b.h;
                }
            }
        }

        let maxRight = 0;
        let maxBottom = 0;
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            const b = estBounds.get(child);
            const r = (b && b.cr > 0) ? b.cr + lp.marginRight : (child.measuredWidth + lp.marginLeft + lp.marginRight);
            const bot = (b && b.cb > 0) ? b.cb + lp.marginBottom : (child.measuredHeight + lp.marginTop + lp.marginBottom);
            maxRight = Math.max(maxRight, r);
            maxBottom = Math.max(maxBottom, bot);
        }

        maxRight += this.paddingRight;
        maxBottom += this.paddingBottom;

        const w = View.resolveSize(Math.max(maxRight, this.minWidth), widthMeasureSpec);
        const h = View.resolveSize(Math.max(maxBottom, this.minHeight), heightMeasureSpec);
        this.setMeasuredDimension(w, h);
    }

    onLayout(changed, l, t, r, b) {
        const parentLeft = this.paddingLeft;
        const parentRight = this.getWidth() - this.paddingRight;
        const parentTop = this.paddingTop;
        const parentBottom = this.getHeight() - this.paddingBottom;

        const childBounds = new Map();

        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const lp = child.layoutParams;
            const w = child.measuredWidth;
            const h = child.measuredHeight;

            let cl = parentLeft + lp.marginLeft;
            let ct = parentTop + lp.marginTop;
            let cr = cl + w;
            let cb = ct + h;

            childBounds.set(child, { cl, ct, cr, cb, w, h, lp });
        }

        // Iterative relaxation (3 passes) over children for forward/backward sibling dependencies
        for (let pass = 0; pass < 3; pass++) {
            for (const child of this.children) {
                if (child.visibility === GONE) continue;
                const bounds = childBounds.get(child);
                const lp = child.layoutParams;
                const c = lp.constraints || {};

                let leftAnchor = null;
                let rightAnchor = null;
                let topAnchor = null;
                let bottomAnchor = null;

                // Horizontal Start / Left
                const startToStart = c.layout_constraintStart_toStartOf ?? c.layout_constraintLeft_toLeftOf;
                if (startToStart === 'parent' || startToStart === 0 || startToStart === -1) {
                    leftAnchor = parentLeft;
                } else if (startToStart !== undefined && startToStart !== null) {
                    const t = this.findViewById(startToStart);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) leftAnchor = tb.cl;
                }

                const startToEnd = c.layout_constraintStart_toEndOf ?? c.layout_constraintLeft_toRightOf;
                if (startToEnd !== undefined && startToEnd !== null) {
                    const t = this.findViewById(startToEnd);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) leftAnchor = tb.cr;
                }

                // Horizontal End / Right
                const endToEnd = c.layout_constraintEnd_toEndOf ?? c.layout_constraintRight_toRightOf;
                if (endToEnd === 'parent' || endToEnd === 0 || endToEnd === -1) {
                    rightAnchor = parentRight;
                } else if (endToEnd !== undefined && endToEnd !== null) {
                    const t = this.findViewById(endToEnd);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) rightAnchor = tb.cr;
                }

                const endToStart = c.layout_constraintEnd_toStartOf ?? c.layout_constraintRight_toLeftOf;
                if (endToStart !== undefined && endToStart !== null) {
                    const t = this.findViewById(endToStart);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) rightAnchor = tb.cl;
                }

                // Vertical Top
                const topToTop = c.layout_constraintTop_toTopOf;
                if (topToTop === 'parent' || topToTop === 0 || topToTop === -1) {
                    topAnchor = parentTop;
                } else if (topToTop !== undefined && topToTop !== null) {
                    const t = this.findViewById(topToTop);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) topAnchor = tb.ct;
                }

                const topToBottom = c.layout_constraintTop_toBottomOf;
                if (topToBottom !== undefined && topToBottom !== null) {
                    const t = this.findViewById(topToBottom);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) topAnchor = tb.cb;
                }

                // Vertical Bottom
                const bottomToBottom = c.layout_constraintBottom_toBottomOf;
                if (bottomToBottom === 'parent' || bottomToBottom === 0 || bottomToBottom === -1) {
                    bottomAnchor = parentBottom;
                } else if (bottomToBottom !== undefined && bottomToBottom !== null) {
                    const t = this.findViewById(bottomToBottom);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) bottomAnchor = tb.cb;
                }

                const bottomToTop = c.layout_constraintBottom_toTopOf;
                if (bottomToTop !== undefined && bottomToTop !== null) {
                    const t = this.findViewById(bottomToTop);
                    const tb = t ? childBounds.get(t) : null;
                    if (tb) bottomAnchor = tb.ct;
                }

                // Apply horizontal positions
                if (leftAnchor !== null && rightAnchor !== null) {
                    const availW = Math.max(0, rightAnchor - leftAnchor - lp.marginLeft - lp.marginRight);
                    if (lp.width === 0 /* MATCH_CONSTRAINT */) {
                        bounds.cl = leftAnchor + lp.marginLeft;
                        bounds.cr = Math.max(bounds.cl, rightAnchor - lp.marginRight);
                        bounds.w = bounds.cr - bounds.cl;
                    } else {
                        const bias = (typeof c.layout_constraintHorizontal_bias === 'number' && !isNaN(c.layout_constraintHorizontal_bias)) ? c.layout_constraintHorizontal_bias : 0.5;
                        bounds.cl = leftAnchor + lp.marginLeft + Math.round((availW - bounds.w) * bias);
                        bounds.cr = bounds.cl + bounds.w;
                    }
                } else if (leftAnchor !== null) {
                    bounds.cl = leftAnchor + lp.marginLeft;
                    bounds.cr = bounds.cl + bounds.w;
                } else if (rightAnchor !== null) {
                    bounds.cr = rightAnchor - lp.marginRight;
                    bounds.cl = bounds.cr - bounds.w;
                }

                // Apply vertical positions
                if (topAnchor !== null && bottomAnchor !== null) {
                    const availH = Math.max(0, bottomAnchor - topAnchor - lp.marginTop - lp.marginBottom);
                    if (lp.height === 0 /* MATCH_CONSTRAINT */) {
                        bounds.ct = topAnchor + lp.marginTop;
                        bounds.cb = Math.max(bounds.ct, bottomAnchor - lp.marginBottom);
                        bounds.h = bounds.cb - bounds.ct;
                    } else {
                        const bias = (typeof c.layout_constraintVertical_bias === 'number' && !isNaN(c.layout_constraintVertical_bias)) ? c.layout_constraintVertical_bias : 0.5;
                        bounds.ct = topAnchor + lp.marginTop + Math.round((availH - bounds.h) * bias);
                        bounds.cb = bounds.ct + bounds.h;
                    }
                } else if (topAnchor !== null) {
                    bounds.ct = topAnchor + lp.marginTop;
                    bounds.cb = bounds.ct + bounds.h;
                } else if (bottomAnchor !== null) {
                    bounds.cb = bottomAnchor - lp.marginBottom;
                    bounds.ct = bounds.cb - bounds.h;
                }

                if (bounds.cr < bounds.cl) bounds.cr = bounds.cl;
                if (bounds.cb < bounds.ct) bounds.cb = bounds.ct;
            }
        }

        // Apply final layout
        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const bnd = childBounds.get(child);
            if (bnd) {
                child.layout(bnd.cl, bnd.ct, bnd.cr, bnd.cb);
            }
        }
    }
}

/**
 * ScrollView: Vertical viewport with unbounded child measurement.
 */
export class ScrollView extends FrameLayout {
    constructor(layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT)) {
        super(layoutParams);
        this.scrollY = 0;
        this.scrollX = 0;
        this.maxScrollY = 0;
    }

    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        if (this.children.length > 0) {
            const child = this.children[0];
            const lp = child.layoutParams || new LayoutParams();
            const childWSpec = ViewGroup.getChildMeasureSpec(
                widthMeasureSpec,
                this.paddingLeft + this.paddingRight + lp.marginLeft + lp.marginRight,
                lp.width
            );
            // Measure child with unbounded vertical height or explicit height
            const childHSpec = lp.height >= 0
                ? MeasureSpec.makeMeasureSpec(lp.height, MeasureSpec.EXACTLY)
                : MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
            child.measure(childWSpec, childHSpec);

            const w = View.getDefaultSize(child.measuredWidth + this.paddingLeft + this.paddingRight + lp.marginLeft + lp.marginRight, widthMeasureSpec);
            const h = View.getDefaultSize(this.paddingTop + this.paddingBottom, heightMeasureSpec);
            this.setMeasuredDimension(w, h);

            this.maxScrollY = Math.max(0, child.measuredHeight - (h - this.paddingTop - this.paddingBottom));
        } else {
            super.onMeasure(widthMeasureSpec, heightMeasureSpec);
        }
    }

    scrollTo(x, y) {
        this.scrollX = Math.max(0, x | 0);
        this.scrollY = Math.max(0, Math.min(this.maxScrollY, y | 0));
    }

    scrollBy(dx, dy) {
        this.scrollTo(this.scrollX + dx, this.scrollY + dy);
    }
}

/**
 * RecyclerView / ListView: Adapter-based list view.
 */
export class RecyclerView extends ViewGroup {
    constructor(layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT)) {
        super(layoutParams);
        this.adapter = null;
        this.layoutManager = 'LinearLayoutManager';
    }

    setAdapter(adapter) {
        this.adapter = adapter;
        this.removeAllViews();
        if (adapter && typeof adapter.getItemCount === 'function') {
            const count = adapter.getItemCount();
            for (let i = 0; i < count; i++) {
                const holder = adapter.onCreateViewHolder(this, 0);
                adapter.onBindViewHolder(holder, i);
                if (holder.itemView) this.addView(holder.itemView);
            }
        }
    }

    setLayoutManager(lm) {
        this.layoutManager = lm;
    }

    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        let totalH = 0;
        let maxW = 0;

        for (const child of this.children) {
            if (child.visibility !== GONE) {
                this.measureChild(child, widthMeasureSpec, heightMeasureSpec);
                totalH += child.measuredHeight;
                maxW = Math.max(maxW, child.measuredWidth);
            }
        }

        totalH += this.paddingTop + this.paddingBottom;
        maxW += this.paddingLeft + this.paddingRight;

        const w = View.getDefaultSize(maxW, widthMeasureSpec);
        const h = View.getDefaultSize(totalH, heightMeasureSpec);
        this.setMeasuredDimension(w, h);
    }

    onLayout(changed, l, t, r, b) {
        let curY = this.top + this.paddingTop;
        const parentLeft = this.left + this.paddingLeft;
        const parentRight = this.right - this.paddingRight;

        for (const child of this.children) {
            if (child.visibility === GONE) continue;
            const w = child.measuredWidth;
            const h = child.measuredHeight;
            child.layout(parentLeft, curY, parentRight, curY + h);
            curY += h;
        }
    }
}

// -----------------------------------------------------------------------------
// 6. Core Widgets
// -----------------------------------------------------------------------------

/**
 * TextView: Text rendering and font metric calculation.
 */
export class TextView extends View {
    constructor(text = '', layoutParams = new LayoutParams(WRAP_CONTENT, WRAP_CONTENT)) {
        super(layoutParams);
        this.text = String(text);
        this.textSize = 14; // sp / dp default
        this.textColor = '#FFFFFF';
        this.typeface = 'sans-serif';
        this.textStyle = 'normal';
        this.textAlignment = 'start';
        this.gravity = 0;
        this.maxLines = 0;
        this.lines = 0;
        this.ellipsize = 0; // 0=none, 1=start, 2=middle, 3=end
        this.lineHeight = 0;
    }

    setText(text) {
        this.text = text !== null && text !== undefined ? String(text) : '';
    }

    getText() {
        return this.text;
    }

    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        const text = this.text || '';
        const fontSize = this.textSize || 14;

        // Approximate font metrics: width = chars * 0.58 * fontSize
        const charWidth = fontSize * 0.58;
        const singleLineH = Math.round(fontSize * 1.3);

        const wMode = MeasureSpec.getMode(widthMeasureSpec);
        const wSize = MeasureSpec.getSize(widthMeasureSpec);
        const maxAvailW = wMode === MeasureSpec.UNSPECIFIED ? Infinity : Math.max(0, wSize - this.paddingLeft - this.paddingRight);

        let contentW = 0;
        let contentH = singleLineH;

        if (text.length > 0) {
            const rawW = Math.round(text.length * charWidth);
            if (rawW <= maxAvailW || maxAvailW === Infinity) {
                contentW = rawW;
                contentH = singleLineH;
            } else {
                // Multi-line wrap
                const charsPerLine = Math.max(1, Math.floor(maxAvailW / charWidth));
                let lineCount = Math.ceil(text.length / charsPerLine);
                if (this.maxLines > 0) lineCount = Math.min(lineCount, this.maxLines);
                if (this.lines > 0) lineCount = this.lines;
                contentW = maxAvailW;
                contentH = lineCount * singleLineH;
            }
        }

        contentW += this.paddingLeft + this.paddingRight;
        contentH += this.paddingTop + this.paddingBottom;

        let w = contentW;
        if (wMode === MeasureSpec.EXACTLY) w = wSize;
        else if (wMode === MeasureSpec.AT_MOST) w = Math.min(contentW, wSize);

        const hMode = MeasureSpec.getMode(heightMeasureSpec);
        const hSize = MeasureSpec.getSize(heightMeasureSpec);
        let h = contentH;
        if (hMode === MeasureSpec.EXACTLY) h = hSize;
        else if (hMode === MeasureSpec.AT_MOST) h = Math.min(contentH, hSize);

        this.setMeasuredDimension(Math.max(w, this.minWidth), Math.max(h, this.minHeight));
    }

    onDraw(ctx) {
        const text = this.text;
        if (!text || !ctx) return;

        const fontSize = this.textSize || 14;
        ctx.font = `${this.textStyle !== 'normal' ? this.textStyle + ' ' : ''}${fontSize}px ${this.typeface}`;
        ctx.fillStyle = this.textColor || '#FFFFFF';

        const availW = this.getWidth() - this.paddingLeft - this.paddingRight;
        const availH = this.getHeight() - this.paddingTop - this.paddingBottom;

        let drawX = this.left + this.paddingLeft;
        let drawY = this.top + this.paddingTop + Math.round(availH / 2);

        if (this.textAlignment === 'center' || (this.gravity & 0x07) === 1 || this.gravity === 17) {
            ctx.textAlign = 'center';
            drawX = this.left + this.paddingLeft + Math.round(availW / 2);
        } else if (this.textAlignment === 'end' || (this.gravity & 0x07) === 5) {
            ctx.textAlign = 'right';
            drawX = this.right - this.paddingRight;
        } else {
            ctx.textAlign = 'left';
        }

        const singleLineH = Math.round(fontSize * 1.3);
        const charWidth = fontSize * 0.58;
        const charsPerLine = Math.max(1, Math.floor(availW / charWidth));

        if (this.maxLines > 1 || this.lines > 1 || (text.length > charsPerLine && availH > singleLineH * 1.5)) {
            // Multi-line text drawing
            ctx.textBaseline = 'top';
            let lineCount = Math.ceil(text.length / charsPerLine);
            if (this.maxLines > 0) lineCount = Math.min(lineCount, this.maxLines);
            if (this.lines > 0) lineCount = this.lines;

            for (let i = 0; i < lineCount; i++) {
                const start = i * charsPerLine;
                let lineText = text.substring(start, start + charsPerLine);
                if (i === lineCount - 1 && start + charsPerLine < text.length) {
                    lineText = lineText.substring(0, Math.max(0, lineText.length - 3)) + '...';
                }
                const lineY = this.top + this.paddingTop + (i * singleLineH);
                if (ctx.fillText) {
                    ctx.fillText(lineText, drawX, lineY, availW > 0 ? availW : undefined);
                }
            }
        } else {
            // Single line text drawing
            ctx.textBaseline = 'middle';
            if (ctx.fillText) {
                ctx.fillText(text, drawX, drawY, availW > 0 ? availW : undefined);
            }
        }
    }
}

export const SCALE_TYPE_FIT_XY = 1;
export const SCALE_TYPE_FIT_CENTER = 3;
export const SCALE_TYPE_CENTER_CROP = 6;
export const SCALE_TYPE_CENTER_INSIDE = 7;

/**
 * VectorDrawable: Android AXML Vector (<vector>) path renderer.
 */
export class VectorDrawable {
    constructor(width = 24, height = 24, viewportWidth = 24, viewportHeight = 24, paths = [], tint = null) {
        this.width = width;
        this.height = height;
        this.viewportWidth = viewportWidth || width || 24;
        this.viewportHeight = viewportHeight || height || 24;
        this.paths = paths;
        this.tint = tint;
    }

    static fromXmlAst(node, resResolver = null) {
        if (!node || node.tag !== 'vector') return null;
        const attrs = node.attrs || {};
        const rawAttrs = node.rawAttrs || [];

        let width = 24;
        let height = 24;
        let viewportWidth = 24;
        let viewportHeight = 24;
        let tint = null;

        for (const raw of rawAttrs) {
            const { name, dataType, data } = raw;
            if (name === 'width' || name === 'layout_width') width = TypedValue.complexToDimension(data) || 24;
            else if (name === 'height' || name === 'layout_height') height = TypedValue.complexToDimension(data) || 24;
            else if (name === 'viewportWidth') viewportWidth = TypedValue.decodeValue(dataType, data) || data || 24;
            else if (name === 'viewportHeight') viewportHeight = TypedValue.decodeValue(dataType, data) || data || 24;
            else if (name === 'tint') tint = resResolver ? resResolver.resolveColor(data) : TypedValue.decodeColor(data, dataType);
        }

        if (attrs.viewportWidth) viewportWidth = parseFloat(attrs.viewportWidth) || viewportWidth;
        if (attrs.viewportHeight) viewportHeight = parseFloat(attrs.viewportHeight) || viewportHeight;
        if (attrs.width) width = parseFloat(attrs.width) || width;
        if (attrs.height) height = parseFloat(attrs.height) || height;
        if (attrs.tint) tint = attrs.tint;

        const paths = [];
        const extractPaths = (parent) => {
            if (!parent || !parent.children) return;
            for (const child of parent.children) {
                if (child.tag === 'path') {
                    let pathData = '';
                    let fillColor = null;
                    let strokeColor = null;
                    let strokeWidth = 0;
                    let fillAlpha = 1.0;
                    let strokeAlpha = 1.0;

                    for (const r of (child.rawAttrs || [])) {
                        if (r.name === 'pathData') pathData = r.rawVal || '';
                        else if (r.name === 'fillColor') fillColor = resResolver ? resResolver.resolveColor(r.data) : TypedValue.decodeColor(r.data, r.dataType);
                        else if (r.name === 'strokeColor') strokeColor = resResolver ? resResolver.resolveColor(r.data) : TypedValue.decodeColor(r.data, r.dataType);
                        else if (r.name === 'strokeWidth') strokeWidth = (r.dataType === 0x10 || r.dataType === 0x11 || r.dataType === 0x04) ? r.data : (TypedValue.complexToDimension(r.data) || r.data || 0);
                        else if (r.name === 'fillAlpha') fillAlpha = TypedValue.decodeValue(r.dataType, r.data) || 1.0;
                        else if (r.name === 'strokeAlpha') strokeAlpha = TypedValue.decodeValue(r.dataType, r.data) || 1.0;
                    }

                    if (child.attrs) {
                        if (child.attrs.pathData) pathData = child.attrs.pathData;
                        if (child.attrs.fillColor) fillColor = child.attrs.fillColor;
                        if (child.attrs.strokeColor) strokeColor = child.attrs.strokeColor;
                        if (child.attrs.strokeWidth) strokeWidth = parseFloat(child.attrs.strokeWidth) || strokeWidth;
                    }

                    paths.push({ pathData, fillColor, strokeColor, strokeWidth, fillAlpha, strokeAlpha });
                } else if (child.children) {
                    extractPaths(child);
                }
            }
        };

        extractPaths(node);
        return new VectorDrawable(width, height, viewportWidth, viewportHeight, paths, tint);
    }

    draw(ctx, x, y, targetWidth, targetHeight) {
        if (!ctx) return;
        ctx.save?.();
        ctx.translate(x, y);
        const scaleX = targetWidth / (this.viewportWidth || 24);
        const scaleY = targetHeight / (this.viewportHeight || 24);
        ctx.scale(scaleX, scaleY);

        for (const p of this.paths) {
            if (!p.pathData) continue;
            try {
                let pathObj = null;
                if (typeof Path2D !== 'undefined') {
                    pathObj = new Path2D(p.pathData);
                }
                const fill = this.tint || p.fillColor;
                if (fill && fill !== 'none' && fill !== 'transparent') {
                    ctx.fillStyle = fill;
                    if (p.fillAlpha < 1.0) {
                        const oldAlpha = ctx.globalAlpha;
                        ctx.globalAlpha *= p.fillAlpha;
                        if (pathObj && ctx.fill) ctx.fill(pathObj);
                        else if (ctx.fill) ctx.fill();
                        ctx.globalAlpha = oldAlpha;
                    } else {
                        if (pathObj && ctx.fill) ctx.fill(pathObj);
                        else if (ctx.fill) ctx.fill();
                    }
                }
                if (p.strokeColor && p.strokeWidth > 0) {
                    ctx.strokeStyle = p.strokeColor;
                    ctx.lineWidth = p.strokeWidth;
                    if (pathObj && ctx.stroke) ctx.stroke(pathObj);
                    else if (ctx.stroke) ctx.stroke();
                }
            } catch (err) {}
        }
        ctx.restore?.();
    }
}

/**
 * BitmapDrawable: PNG / WebP / JPEG image bitmap renderer.
 */
export class BitmapDrawable {
    constructor(bitmapOrBuffer, width = 48, height = 48) {
        this.bitmap = bitmapOrBuffer;
        this.width = width;
        this.height = height;
    }

    draw(ctx, x, y, targetWidth, targetHeight) {
        if (!ctx || !this.bitmap) return;
        if (ctx.drawImage) {
            try {
                ctx.drawImage(this.bitmap, x, y, targetWidth, targetHeight);
            } catch (e) {}
        }
    }
}

/**
 * ImageView: Renders drawable assets, vector paths, and bitmap resources.
 */
export class ImageView extends View {
    static FIT_XY = SCALE_TYPE_FIT_XY;
    static FIT_CENTER = SCALE_TYPE_FIT_CENTER;
    static CENTER_CROP = SCALE_TYPE_CENTER_CROP;
    static CENTER_INSIDE = SCALE_TYPE_CENTER_INSIDE;

    constructor(layoutParams = new LayoutParams(WRAP_CONTENT, WRAP_CONTENT)) {
        super(layoutParams);
        this.src = null;
        this.drawable = null;
        this.scaleType = SCALE_TYPE_FIT_CENTER;
        this.tint = null;
        this.contentDescription = '';
        this.intrinsicWidth = 48;
        this.intrinsicHeight = 48;
    }

    setImageResource(src) {
        this.src = src;
    }

    setDrawable(drawable) {
        this.drawable = drawable;
        if (drawable) {
            if (typeof drawable.width === 'number' && drawable.width > 0) {
                this.intrinsicWidth = drawable.width;
            }
            if (typeof drawable.height === 'number' && drawable.height > 0) {
                this.intrinsicHeight = drawable.height;
            }
        }
    }

    setImageBitmap(bitmap) {
        this.setDrawable(new BitmapDrawable(bitmap));
    }

    onMeasure(widthMeasureSpec, heightMeasureSpec) {
        let w = this.intrinsicWidth + this.paddingLeft + this.paddingRight;
        let h = this.intrinsicHeight + this.paddingTop + this.paddingBottom;

        const measuredW = View.resolveSize(Math.max(w, this.minWidth), widthMeasureSpec);
        const measuredH = View.resolveSize(Math.max(h, this.minHeight), heightMeasureSpec);
        this.setMeasuredDimension(measuredW, measuredH);
    }

    onDraw(ctx) {
        if (!ctx) return;
        const w = this.getWidth() - this.paddingLeft - this.paddingRight;
        const h = this.getHeight() - this.paddingTop - this.paddingBottom;
        const x = this.left + this.paddingLeft;
        const y = this.top + this.paddingTop;

        if (this.drawable && typeof this.drawable.draw === 'function') {
            this.drawable.draw(ctx, x, y, w, h);
        } else if (this.text) {
            // Emoji or text icon glyph rendering
            ctx.font = `${Math.round(h * 0.6)}px sans-serif`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText(this.text, x + Math.round(w / 2), y + Math.round(h / 2));
        } else if (this.tint) {
            ctx.fillStyle = this.tint;
            ctx.fillRect(x, y, w, h);
        } else {
            // Placeholder drawable box
            ctx.fillStyle = 'rgba(255, 255, 255, 0.15)';
            ctx.fillRect(x, y, w, h);
        }
    }
}

/**
 * Button: Material Design 3 interactive button widget.
 */
export class Button extends TextView {
    constructor(text = '', layoutParams = new LayoutParams(WRAP_CONTENT, WRAP_CONTENT)) {
        super(text, layoutParams);
        this.isClickable = true;
        this.isFocusable = true;
        this.textSize = 14;
        this.textColor = '#FFFFFF';
        this.backgroundColor = '#6750A4'; // MD3 primary color
        this.cornerRadius = 20; // Pill corner radius
        this.setPadding(16, 10, 16, 10);
        this.gravity = 17; // CENTER
    }

    drawBackground(ctx, w, h) {
        if (!ctx) return;
        const x = this.left;
        const y = this.top;
        const r = this.cornerRadius || 8;

        ctx.save?.();
        ctx.fillStyle = this.isPressed ? '#4F378B' : (this.backgroundColor || '#6750A4');
        if (ctx.beginPath) {
            ctx.beginPath();
            if (ctx.roundRect) {
                ctx.roundRect(x, y, w, h, r);
            } else if (ctx.rect) {
                ctx.rect(x, y, w, h);
            }
            if (ctx.fill) ctx.fill();
        } else if (ctx.fillRect) {
            ctx.fillRect(x, y, w, h);
        }
        ctx.restore?.();
    }
}

/**
 * EditText: Interactive text input widget with hint support.
 */
export class EditText extends TextView {
    constructor(text = '', layoutParams = new LayoutParams(MATCH_PARENT, WRAP_CONTENT)) {
        super(text, layoutParams);
        this.isClickable = true;
        this.isFocusable = true;
        this.hint = '';
        this.hintColor = '#94a3b8';
        this.textSize = 14;
        this.textColor = '#f8fafc';
        this.backgroundColor = '#1e293b';
        this.cornerRadius = 8;
        this.setPadding(14, 10, 14, 10);
    }

    setHint(hint) {
        this.hint = hint;
    }

    onDraw(ctx) {
        if (!ctx) return;
        const displayTxt = this.text || this.hint;
        if (!displayTxt) return;

        const fontSize = this.textSize || 14;
        ctx.font = `${this.textStyle !== 'normal' ? this.textStyle + ' ' : ''}${fontSize}px ${this.typeface}`;
        ctx.fillStyle = this.text ? (this.textColor || '#FFFFFF') : (this.hintColor || '#94a3b8');
        ctx.textBaseline = 'middle';

        const availW = this.getWidth() - this.paddingLeft - this.paddingRight;
        const availH = this.getHeight() - this.paddingTop - this.paddingBottom;
        let drawX = this.left + this.paddingLeft;
        let drawY = this.top + this.paddingTop + Math.round(availH / 2);

        if (ctx.fillText) {
            ctx.fillText(displayTxt, drawX, drawY, availW > 0 ? availW : undefined);
        }
    }
}

// -----------------------------------------------------------------------------
// 7. Binary XML Attribute Map & LayoutInflater
// -----------------------------------------------------------------------------

export const ANDROID_ATTR_IDS = {
    0x01010001: 'label',
    0x01010002: 'icon',
    0x01010003: 'name',
    0x01010095: 'textSize',
    0x01010097: 'textStyle',
    0x01010098: 'textColor',
    0x010100ab: 'ellipsize',
    0x010100af: 'gravity',
    0x010100b3: 'layout_gravity',
    0x010100c4: 'orientation',
    0x010100d0: 'id',
    0x010100d4: 'background',
    0x010100d5: 'padding',
    0x010100d6: 'paddingLeft',
    0x010100d7: 'paddingTop',
    0x010100d8: 'paddingRight',
    0x010100d9: 'paddingBottom',
    0x010100da: 'focusable',
    0x010100dc: 'visibility',
    0x010100e5: 'clickable',
    0x010100f4: 'layout_width',
    0x010100f5: 'layout_height',
    0x010100f6: 'layout_weight',
    0x010100f7: 'layout_margin',
    0x010100f8: 'layout_marginLeft',
    0x010100f9: 'layout_marginTop',
    0x010100fa: 'layout_marginRight',
    0x010100fb: 'layout_marginBottom',
    0x01010119: 'src',
    0x0101011d: 'scaleType',
    0x0101014f: 'text',
    0x01010153: 'maxLines',
    0x01010154: 'lines',
    0x01010180: 'layout_toLeftOf',
    0x01010181: 'layout_toRightOf',
    0x01010182: 'layout_above',
    0x01010183: 'layout_below',
    0x01010184: 'layout_alignParentTop',
    0x01010185: 'layout_alignParentBottom',
    0x01010186: 'layout_alignParentLeft',
    0x01010187: 'layout_alignParentRight',
    0x01010188: 'layout_alignTop',
    0x01010189: 'layout_alignBottom',
    0x0101018a: 'layout_alignLeft',
    0x0101018b: 'layout_alignRight',
    0x0101018c: 'layout_alignParentBottom',
    0x0101018d: 'layout_centerInParent',
    0x0101018e: 'layout_centerHorizontal',
    0x0101018f: 'layout_centerVertical',
    0x010101e1: 'tint',
    0x010101e5: 'contentDescription',
    0x0101031f: 'alpha',
    0x0101038c: 'elevation',
    0x010103a5: 'layout_alignParentStart',
    0x010103a6: 'layout_alignParentEnd',
    0x010103a7: 'layout_alignStart',
    0x010103a8: 'layout_alignEnd',
    0x010103a9: 'layout_toStartOf',
    0x010103aa: 'layout_toEndOf',
    0x010103b3: 'layout_marginStart',
    0x010103b4: 'layout_marginEnd',
    0x010103b7: 'paddingStart',
    0x010103b8: 'paddingEnd',
    0x01010155: 'height',
    0x01010159: 'width',
    0x01010402: 'viewportWidth',
    0x01010403: 'viewportHeight',
    0x01010404: 'fillColor',
    0x01010405: 'pathData',
    0x01010406: 'strokeColor',
    0x01010407: 'strokeWidth',
    0x0101040a: 'strokeAlpha',
    0x0101040b: 'fillAlpha'
};

/**
 * Pure binary AXML layout tree decoder.
 */
function parseAxmlLayoutBuffer(buffer) {
    let bytes;
    if (buffer instanceof ArrayBuffer) bytes = new Uint8Array(buffer);
    else if (buffer && buffer.buffer) bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    else return null;

    if (bytes.byteLength < 8) return null;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (view.getUint16(0, true) !== 0x0003 /* RES_XML_TYPE */) return null;

    let stringPool = [];
    let resMap = [];
    const textDecoder = new TextDecoder('utf-8');

    // Pass 1: String pool & Resource Map
    let pos = 8;
    while (pos + 8 <= bytes.byteLength) {
        const chunkType = view.getUint16(pos, true);
        const chunkSize = view.getUint32(pos + 4, true);
        if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) break;

        if (chunkType === 0x0001 /* RES_STRING_POOL_TYPE */) {
            const count = view.getUint32(pos + 8, true);
            const stringsStart = view.getUint32(pos + 20, true);
            const isUtf8 = (view.getUint32(pos + 16, true) & (1 << 8)) !== 0;
            for (let i = 0; i < count; i++) {
                const off = view.getUint32(pos + 28 + i * 4, true);
                const abs = pos + stringsStart + off;
                if (isUtf8) {
                    let cursor = abs;
                    if (bytes[cursor] & 0x80) cursor += 2; else cursor += 1;
                    let len = bytes[cursor] & 0x80 ? (((bytes[cursor] & 0x7F) << 8) | bytes[cursor - 1]) : bytes[cursor];
                    cursor += (bytes[cursor] & 0x80) ? 2 : 1;
                    stringPool.push(textDecoder.decode(bytes.subarray(cursor, cursor + len)).replace(/\0+$/, ''));
                } else {
                    let cursor = abs;
                    const charLen = view.getUint16(cursor, true);
                    cursor += 2;
                    const u16 = [];
                    for (let c = 0; c < charLen; c++) u16.push(view.getUint16(cursor + c * 2, true));
                    stringPool.push(String.fromCharCode(...u16));
                }
            }
        } else if (chunkType === 0x0180 /* RES_XML_RESOURCE_MAP_TYPE */) {
            const count = (chunkSize - 8) / 4;
            for (let i = 0; i < count; i++) {
                resMap.push(view.getUint32(pos + 8 + i * 4, true));
            }
        }
        pos += chunkSize;
    }

    // Pass 2: Tags & Attributes
    pos = 8;
    const stack = [];
    let root = null;

    while (pos + 8 <= bytes.byteLength) {
        const chunkType = view.getUint16(pos, true);
        const chunkSize = view.getUint32(pos + 4, true);
        if (chunkSize < 8 || pos + chunkSize > bytes.byteLength) break;

        if (chunkType === 0x0102 /* RES_XML_START_ELEMENT_TYPE */) {
            const tagIdx = view.getUint32(pos + 20, true);
            const tagName = stringPool[tagIdx] || `tag_${tagIdx}`;
            const attrCount = view.getUint16(pos + 28, true);
            const attrs = {};
            const rawAttrs = [];

            let attrOff = pos + 36;
            for (let i = 0; i < attrCount; i++) {
                if (attrOff + 20 <= pos + chunkSize) {
                    const nameIdx = view.getUint32(attrOff + 4, true);
                    const rawIdx = view.getUint32(attrOff + 8, true);
                    const dataType = bytes[attrOff + 15];
                    const data = view.getUint32(attrOff + 16, true);

                    let attrName = stringPool[nameIdx] || `attr_${nameIdx}`;
                    const resId = resMap[nameIdx] || 0;
                    if (ANDROID_ATTR_IDS[resId]) {
                        attrName = ANDROID_ATTR_IDS[resId];
                    }

                    const rawVal = rawIdx !== 0xFFFFFFFF ? stringPool[rawIdx] : null;
                    rawAttrs.push({ name: attrName, resId, dataType, data, rawVal });
                    attrs[attrName] = rawVal !== null ? rawVal : data;

                    attrOff += 20;
                }
            }

            const node = { tag: tagName, attrs, rawAttrs, children: [] };
            if (stack.length > 0) stack[stack.length - 1].children.push(node);
            else root = node;
            stack.push(node);
        } else if (chunkType === 0x0103 /* RES_XML_END_ELEMENT_TYPE */) {
            stack.pop();
        }
        pos += chunkSize;
    }

    return root;
}

export class LayoutInflater {
    /**
     * Inflates a binary XML layout buffer or AST into an authentic in-memory View hierarchy.
     * @param {ArrayBuffer|Uint8Array|object} xmlBufferOrTree - Binary layout buffer or decoded AST.
     * @param {object} [resourceResolver] - ArscResourceTable for resolving IDs and resources.
     * @param {ViewGroup} [parent=null] - Optional parent container to attach to.
     * @param {boolean} [attachToRoot=true] - Whether to attach to root parent.
     * @returns {View} Root inflated View.
     */
    static inflate(xmlBufferOrTree, resourceResolver = null, parent = null, attachToRoot = true, density = 1.0) {
        let ast = xmlBufferOrTree;
        if (xmlBufferOrTree instanceof ArrayBuffer || (xmlBufferOrTree && xmlBufferOrTree.buffer)) {
            ast = parseAxmlLayoutBuffer(xmlBufferOrTree);
        }

        if (!ast || typeof ast !== 'object') {
            return new View();
        }

        const root = LayoutInflater._inflateNode(ast, resourceResolver, density) || new View();
        if (parent && root && attachToRoot) {
            parent.addView(root);
        }
        return root;
    }

    static _createViewForTag(tag) {
        const cleanTag = tag.includes('.') ? tag.slice(tag.lastIndexOf('.') + 1) : tag;

        switch (cleanTag) {
            case 'FrameLayout':
            case 'SwipeRefreshLayout':
            case 'CoordinatorLayout':
            case 'DrawerLayout':
            case 'CardView':
            case 'MaterialCardView':
            case 'ViewPager':
            case 'ViewPager2':
            case 'AppBarLayout':
            case 'CollapsingToolbarLayout':
                return new FrameLayout();

            case 'LinearLayout':
                return new LinearLayout();

            case 'RelativeLayout':
                return new RelativeLayout();

            case 'ConstraintLayout':
            case 'MotionLayout':
                return new ConstraintLayout();

            case 'ScrollView':
            case 'NestedScrollView':
            case 'HorizontalScrollView':
                return new ScrollView();

            case 'RecyclerView':
            case 'ListView':
            case 'GridView':
                return new RecyclerView();

            case 'TextView':
            case 'AppCompatTextView':
            case 'MaterialTextView':
                return new TextView();

            case 'ImageView':
            case 'AppCompatImageView':
            case 'ImageButton':
            case 'FloatingActionButton':
                return new ImageView();

            case 'Button':
            case 'AppCompatButton':
            case 'MaterialButton':
                return new Button();

            case 'BottomNavigationView':
            case 'NavigationView':
            case 'Toolbar':
            case 'MaterialToolbar':
            case 'ViewGroup':
                return new ViewGroup();

            case 'View':
                return new View();

            default:
                if (cleanTag.endsWith('Layout') || cleanTag.endsWith('View') || cleanTag.endsWith('Container') || cleanTag.endsWith('Panel') || cleanTag.endsWith('Group')) {
                    return new FrameLayout();
                }
                return new View();
        }
    }

    static _inflateNode(node, resResolver, density = 1.0) {
        if (!node || !node.tag) return null;

        const view = LayoutInflater._createViewForTag(node.tag);
        const lp = new LayoutParams();

        const attrs = node.attrs || {};
        const rawAttrs = node.rawAttrs || [];

        // Apply raw attributes with typed value decoding
        for (const raw of rawAttrs) {
            const { name, dataType, data, rawVal } = raw;
            LayoutInflater._applyAttribute(view, lp, name, dataType, data, rawVal, resResolver, density);
        }

        // Apply fallback dictionary attributes if rawAttrs was absent
        if (rawAttrs.length === 0) {
            for (const [name, val] of Object.entries(attrs)) {
                LayoutInflater._applyDictionaryAttribute(view, lp, name, val, resResolver, density);
            }
        }

        view.layoutParams = lp;

        // Inflate children recursively
        if (view instanceof ViewGroup && Array.isArray(node.children)) {
            for (const childNode of node.children) {
                const childView = LayoutInflater._inflateNode(childNode, resResolver, density);
                if (childView) {
                    view.addView(childView);
                }
            }
        }

        return view;
    }

    static _applyAttribute(view, lp, name, dataType, data, rawVal, resResolver, density) {
        // Dimension decoding
        const resolveDimenVal = () => {
            if (dataType === TypedValue.TYPE_DIMENSION) {
                return TypedValue.complexToDimension(data, density);
            }
            if (dataType === TypedValue.TYPE_INT_DEC) {
                return data | 0; // -1 (MATCH_PARENT), -2 (WRAP_CONTENT)
            }
            if (dataType === TypedValue.TYPE_REFERENCE && resResolver) {
                const d = resResolver.resolveDimension(data, density);
                if (d !== null) return d;
            }
            return data;
        };

        // Color decoding
        const resolveColorVal = () => {
            if (dataType >= TypedValue.TYPE_FIRST_COLOR_INT && dataType <= TypedValue.TYPE_LAST_COLOR_INT) {
                return TypedValue.decodeColor(data, dataType);
            }
            if (dataType === TypedValue.TYPE_REFERENCE && resResolver) {
                const c = resResolver.resolveColor(data);
                if (c !== null) return c;
            }
            if (typeof rawVal === 'string' && rawVal.startsWith('#')) return rawVal;
            return TypedValue.decodeColor(data, TypedValue.TYPE_INT_COLOR_ARGB8);
        };

        // String decoding
        const resolveStringVal = () => {
            if (rawVal) return rawVal;
            if (dataType === TypedValue.TYPE_STRING && resResolver) {
                return resResolver.globalStrings[data] || '';
            }
            if (dataType === TypedValue.TYPE_REFERENCE && resResolver) {
                const s = resResolver.resolveString(data);
                if (s !== null) return s;
            }
            return String(data);
        };

        switch (name) {
            case 'id':
                view.id = data >>> 0;
                break;

            case 'layout_width':
                lp.width = resolveDimenVal();
                break;

            case 'layout_height':
                lp.height = resolveDimenVal();
                break;

            case 'layout_weight':
                lp.weight = dataType === TypedValue.TYPE_FLOAT
                    ? TypedValue.decodeValue(dataType, data)
                    : (data / 1.0);
                break;

            case 'orientation':
                if (view instanceof LinearLayout) {
                    view.orientation = (data | 0) === 0 ? HORIZONTAL : VERTICAL;
                }
                break;

            case 'gravity':
                view.gravity = data | 0;
                break;

            case 'layout_gravity':
                lp.gravity = data | 0;
                break;

            case 'layout_margin': {
                const m = resolveDimenVal();
                lp.setMargins(m, m, m, m);
                break;
            }
            case 'layout_marginLeft':
            case 'layout_marginStart':
                lp.marginLeft = resolveDimenVal();
                break;
            case 'layout_marginTop':
                lp.marginTop = resolveDimenVal();
                break;
            case 'layout_marginRight':
            case 'layout_marginEnd':
                lp.marginRight = resolveDimenVal();
                break;
            case 'layout_marginBottom':
                lp.marginBottom = resolveDimenVal();
                break;

            case 'padding': {
                const p = resolveDimenVal();
                view.setPadding(p, p, p, p);
                break;
            }
            case 'paddingLeft':
            case 'paddingStart':
                view.paddingLeft = resolveDimenVal();
                break;
            case 'paddingTop':
                view.paddingTop = resolveDimenVal();
                break;
            case 'paddingRight':
            case 'paddingEnd':
                view.paddingRight = resolveDimenVal();
                break;
            case 'paddingBottom':
                view.paddingBottom = resolveDimenVal();
                break;

            case 'text':
                if (view instanceof TextView) view.setText(resolveStringVal());
                break;

            case 'textSize':
                if (view instanceof TextView) {
                    view.textSize = resolveDimenVal();
                }
                break;

            case 'textColor':
                if (view instanceof TextView) {
                    view.textColor = resolveColorVal();
                }
                break;

            case 'maxLines':
                if (view instanceof TextView) view.maxLines = data | 0;
                break;

            case 'lines':
                if (view instanceof TextView) view.lines = data | 0;
                break;

            case 'ellipsize':
                if (view instanceof TextView) view.ellipsize = data | 0;
                break;

            case 'background':
                view.background = resolveColorVal();
                view.backgroundColor = view.background;
                break;

            case 'src':
                if (view instanceof ImageView) {
                    if (dataType === TypedValue.TYPE_REFERENCE && resResolver) {
                        view.setImageResource(data);
                        if (typeof resResolver.resolveDrawable === 'function') {
                            const d = resResolver.resolveDrawable(data);
                            if (d) {
                                if (d.type === 'color' && d.color) {
                                    view.tint = d.color;
                                } else if (d.data && d.type === 'vector') {
                                    const ast = parseAxmlLayoutBuffer(d.data);
                                    if (ast) {
                                        const vd = VectorDrawable.fromXmlAst(ast, resResolver);
                                        if (vd) view.setDrawable(vd);
                                    }
                                }
                            }
                        }
                    } else if (rawVal) {
                        view.src = rawVal;
                    } else {
                        view.src = data;
                    }
                }
                break;

            case 'tint':
                if (view instanceof ImageView) view.tint = resolveColorVal();
                break;

            case 'scaleType':
                if (view instanceof ImageView) view.scaleType = data | 0;
                break;

            case 'clickable':
                view.isClickable = data !== 0;
                break;

            case 'focusable':
                view.isFocusable = data !== 0;
                break;

            case 'visibility':
                view.visibility = data | 0;
                break;

            case 'contentDescription':
                if (view instanceof ImageView) view.contentDescription = resolveStringVal();
                break;

            default:
                // ConstraintLayout and RelativeLayout rules
                if (name === 'layout_constraintHorizontal_bias' || name === 'layout_constraintVertical_bias') {
                    if (dataType === TypedValue.TYPE_FLOAT) {
                        lp.constraints[name] = TypedValue.decodeValue(dataType, data);
                    } else if (typeof rawVal === 'string' && !isNaN(parseFloat(rawVal))) {
                        lp.constraints[name] = parseFloat(rawVal);
                    } else if (typeof data === 'number') {
                        lp.constraints[name] = TypedValue.complexToFloat(data);
                    }
                } else if (name.startsWith('layout_constraint')) {
                    lp.constraints[name] = data >>> 0;
                } else if (name.startsWith('layout_align') || name.startsWith('layout_to') || name.startsWith('layout_above') || name.startsWith('layout_below') || name.startsWith('layout_center')) {
                    lp.alignRules[name] = data >>> 0;
                }
                break;
        }
    }

    static _applyDictionaryAttribute(view, lp, name, val, resResolver, density) {
        if (typeof val === 'number') {
            LayoutInflater._applyAttribute(view, lp, name, TypedValue.TYPE_INT_DEC, val, null, resResolver, density);
        } else if (typeof val === 'string') {
            if (val.startsWith('@') || val.startsWith('?')) {
                const refId = resResolver ? resResolver.resolveIdentifierRef(val) : null;
                if (refId) {
                    LayoutInflater._applyAttribute(view, lp, name, TypedValue.TYPE_REFERENCE, refId, val, resResolver, density);
                } else {
                    LayoutInflater._applyAttribute(view, lp, name, TypedValue.TYPE_STRING, 0, val, resResolver, density);
                }
            } else {
                LayoutInflater._applyAttribute(view, lp, name, TypedValue.TYPE_STRING, 0, val, resResolver, density);
            }
        }
    }
}

if (typeof window !== 'undefined') {
    window.MeasureSpec = MeasureSpec;
    window.LayoutParams = LayoutParams;
    window.View = View;
    window.ViewGroup = ViewGroup;
    window.FrameLayout = FrameLayout;
    window.LinearLayout = LinearLayout;
    window.RelativeLayout = RelativeLayout;
    window.ConstraintLayout = ConstraintLayout;
    window.ScrollView = ScrollView;
    window.RecyclerView = RecyclerView;
    window.TextView = TextView;
    window.EditText = EditText;
    window.ImageView = ImageView;
    window.VectorDrawable = VectorDrawable;
    window.BitmapDrawable = BitmapDrawable;
    window.Button = Button;
    window.LayoutInflater = LayoutInflater;
}
