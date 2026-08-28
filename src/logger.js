/**
 * src/logger.js - Centralized Structured Logging & In-UI Logcat Buffer
 * 
 * Conforms to Requirement R2, ASD-STE100, and /ponytail simplicity principles.
 */

export const LOG_LEVELS = Object.freeze({
    VERBOSE: 'V',
    DEBUG: 'D',
    INFO: 'I',
    WARN: 'W',
    ERROR: 'E'
});

export const PRIORITY_ORDER = Object.freeze({
    'V': 0,
    'D': 1,
    'I': 2,
    'W': 3,
    'E': 4
});

export const KNOWN_SUBSYSTEMS = Object.freeze({
    v86: 'v86',
    bridge: 'bridge',
    compositor: 'compositor',
    runtime: 'runtime',
    pms: 'pms',
    ams: 'ams',
    wms: 'wms',
    input: 'input',
    audio: 'audio',
    camera: 'camera',
    system: 'system'
});

/**
 * Structured Logger Engine emitting normalized prefixes [v86], [bridge], [compositor]
 */
export class StructuredLogger {
    constructor(options = {}) {
        this.consoleDispatch = options.consoleDispatch !== undefined
            ? Boolean(options.consoleDispatch)
            : (typeof window !== 'undefined');
        this.listeners = new Set();
        this.logs = [];
        this.maxLogHistory = options.maxLogHistory || 5000;
    }

    /**
     * Sanitize metadata safely against circular references
     * @param {any} metadata 
     * @returns {any}
     */
    sanitizeMetadata(metadata) {
        if (metadata === null || metadata === undefined) {
            return null;
        }
        try {
            JSON.stringify(metadata);
            return metadata;
        } catch (_) {
            return { error: 'circular_or_unserializable_metadata' };
        }
    }

    /**
     * Emit a structured log entry
     * @param {string} subsystem 
     * @param {string} level 
     * @param {string} message 
     * @param {object} [metadata] 
     * @returns {object} Structured log entry
     */
    log(subsystem, level, message, metadata = null) {
        const safeSubsystem = ['v86', 'bridge', 'compositor'].includes(subsystem)
            ? subsystem
            : (KNOWN_SUBSYSTEMS[subsystem] ? subsystem : 'unknown');

        const prefix = `[${safeSubsystem}]`;
        const safeLevel = ['V', 'D', 'I', 'W', 'E'].includes(level) ? level : 'I';
        const safeMsg = String(message ?? '');
        const safeMeta = this.sanitizeMetadata(metadata);
        const timestamp = Date.now();
        const formatted = `${prefix} [${safeLevel}] ${safeMsg}`;

        const entry = {
            timestamp,
            prefix,
            subsystem: safeSubsystem,
            level: safeLevel,
            message: safeMsg,
            metadata: safeMeta,
            formatted
        };

        this.logs.push(entry);
        if (this.logs.length > this.maxLogHistory) {
            this.logs.shift();
        }

        if (this.consoleDispatch && typeof console !== 'undefined') {
            this.dispatchToConsole(entry);
        }

        for (const listener of this.listeners) {
            try {
                listener(entry);
            } catch (_) {}
        }

        return entry;
    }

    dispatchToConsole(entry) {
        const metaArg = entry.metadata ? entry.metadata : '';
        switch (entry.level) {
            case 'V':
            case 'D':
                (console.debug || console.log)(entry.formatted, metaArg);
                break;
            case 'I':
                (console.info || console.log)(entry.formatted, metaArg);
                break;
            case 'W':
                (console.warn || console.log)(entry.formatted, metaArg);
                break;
            case 'E':
                (console.error || console.log)(entry.formatted, metaArg);
                break;
            default:
                console.log(entry.formatted, metaArg);
                break;
        }
    }

    v(subsystem, message, metadata) { return this.log(subsystem, 'V', message, metadata); }
    d(subsystem, message, metadata) { return this.log(subsystem, 'D', message, metadata); }
    i(subsystem, message, metadata) { return this.log(subsystem, 'I', message, metadata); }
    w(subsystem, message, metadata) { return this.log(subsystem, 'W', message, metadata); }
    e(subsystem, message, metadata) { return this.log(subsystem, 'E', message, metadata); }

    addListener(fn) {
        if (typeof fn === 'function') this.listeners.add(fn);
    }

    removeListener(fn) {
        this.listeners.delete(fn);
    }

    clear() {
        this.logs = [];
    }
}

/**
 * In-UI Logcat Buffer with 5000-entry max capacity, FIFO eviction, and serial stream buffering
 */
export class LogcatBuffer {
    constructor(maxEntries = 5000) {
        this.maxEntries = maxEntries;
        this.entries = [];
        this.serialBuffer = '';
        this.listeners = new Set();
    }

    /**
     * Format timestamp to MM-DD HH:MM:SS.mmm
     * @param {Date} now 
     * @returns {string}
     */
    static formatTimestamp(now = new Date()) {
        const d = now instanceof Date ? now : new Date(now);
        const month = String(d.getMonth() + 1).padStart(2, '0');
        const day = String(d.getDate()).padStart(2, '0');
        const hours = String(d.getHours()).padStart(2, '0');
        const minutes = String(d.getMinutes()).padStart(2, '0');
        const seconds = String(d.getSeconds()).padStart(2, '0');
        const millis = String(d.getMilliseconds()).padStart(3, '0');
        return `${month}-${day} ${hours}:${minutes}:${seconds}.${millis}`;
    }

    formatTimestamp(now = new Date()) {
        return LogcatBuffer.formatTimestamp(now);
    }

    /**
     * Append logcat record
     */
    append(tag, msg, priority = 'I', pid = 10042, tid = 10042, now = new Date(), metadata = null) {
        const dateObj = now instanceof Date ? now : new Date(now);
        const timeStr = LogcatBuffer.formatTimestamp(dateObj);
        const safePrio = ['V', 'D', 'I', 'W', 'E'].includes(priority) ? priority : 'I';
        const safeTag = String(tag ?? '');
        const safeMsg = String(msg ?? '');
        const formatted = `${timeStr} ${pid} ${tid} ${safePrio} ${safeTag}: ${safeMsg}`;

        const entry = {
            timestamp: dateObj,
            timeStr,
            pid,
            tid,
            priority: safePrio,
            tag: safeTag,
            msg: safeMsg,
            formatted,
            metadata
        };

        this.entries.push(entry);
        if (this.entries.length > this.maxEntries) {
            this.entries.shift(); // Single-item FIFO drop
        }

        for (const listener of this.listeners) {
            try {
                listener(entry, this);
            } catch (_) {}
        }

        if (typeof console !== 'undefined') {
            const consoleMethod = (safePrio === 'E') ? (console.error || console.log) 
                : ((safePrio === 'W') ? (console.warn || console.log) 
                : ((safePrio === 'D' || safePrio === 'V') ? (console.debug || console.log) : (console.info || console.log)));
            consoleMethod(`[${safeTag}] ${safeMsg}`);
        }

        return entry;
    }

    /**
     * Filter logcat records
     */
    filter({ minPriority = 'V', tagQuery = '', msgQuery = '', tag = '', search = '' } = {}) {
        const minRank = PRIORITY_ORDER[minPriority] ?? 0;
        const targetTag = String(tagQuery || tag || '').trim().toLowerCase();
        const targetMsg = String(msgQuery || search || '').trim().toLowerCase();

        return this.entries.filter(e => {
            const rank = PRIORITY_ORDER[e.priority] ?? 0;
            if (rank < minRank) return false;

            if (targetTag && targetTag !== 'all') {
                if (!e.tag.toLowerCase().includes(targetTag)) return false;
            }

            if (targetMsg) {
                const matchMsg = e.msg.toLowerCase().includes(targetMsg);
                const matchTag = e.tag.toLowerCase().includes(targetMsg);
                const matchFmt = e.formatted.toLowerCase().includes(targetMsg);
                if (!matchMsg && !matchTag && !matchFmt) return false;
            }

            return true;
        });
    }

    /**
     * Query alias for filter
     */
    query(opts) {
        return this.filter(opts);
    }

    /**
     * Feed single serial character
     * @param {string} char 
     * @param {function} [onLine] 
     */
    feedSerialChar(char, onLine = null) {
        if (char === '\r') return;
        if (char === '\n') {
            const line = this.serialBuffer.trim();
            this.serialBuffer = '';
            if (line.length > 0) {
                const priority = (line.includes('panic') || line.includes('error') || line.includes('Fatal') || line.includes('Exception') || line.includes('SIGILL')) ? 'E' : 'D';
                const entry = this.append('v86Guest', line, priority);
                if (typeof onLine === 'function') onLine(entry);
            }
        } else {
            this.serialBuffer += char;
        }
    }

    /**
     * Feed serial chunk or string
     * @param {string} text 
     * @param {function} [onLine] 
     */
    feedSerial(text, onLine = null) {
        if (!text) return;
        const lines = text.split(/\r?\n/);
        for (let i = 0; i < lines.length - 1; i++) {
            const line = (this.serialBuffer + lines[i]).trim();
            this.serialBuffer = '';
            if (line.length > 0) {
                const priority = (line.includes('panic') || line.includes('error') || line.includes('Fatal') || line.includes('Exception') || line.includes('SIGILL')) ? 'E' : 'D';
                const entry = this.append('v86Guest', line, priority);
                if (typeof onLine === 'function') onLine(entry);
            }
        }
        this.serialBuffer += lines[lines.length - 1];
    }

    addListener(fn) {
        if (typeof fn === 'function') this.listeners.add(fn);
    }

    removeListener(fn) {
        this.listeners.delete(fn);
    }

    subscribe(fn) {
        this.addListener(fn);
        return () => this.removeListener(fn);
    }

    clear() {
        this.entries = [];
        this.serialBuffer = '';
        for (const listener of this.listeners) {
            try {
                listener(null, this);
            } catch (_) {}
        }
    }
}

// Global default instances & helper exports
export const logger = new StructuredLogger();
export const globalLogcat = new LogcatBuffer(5000);

export function logDebug(subsystem, level, message, metadata = null) {
    const entry = logger.log(subsystem, level, message, metadata);
    const tag = ['v86', 'bridge', 'compositor'].includes(subsystem) ? subsystem : (subsystem || 'System');
    globalLogcat.append(tag, message, entry.level, 10042, 10042, new Date(entry.timestamp), metadata);
    return entry;
}

export function createStructuredLogger(options) {
    return new StructuredLogger(options);
}

// Global hook bindings for WASM bridge and in-browser inspection
if (typeof window !== 'undefined') {
    window.logger = logger;
    window.globalLogcat = globalLogcat;
    window.logDebug = logDebug;
    window.StructuredLogger = StructuredLogger;
    window.LogcatBuffer = LogcatBuffer;
    window.__androidWebGpuLog = (subsystem, level, message, metadata) => logDebug(subsystem, level, message, metadata);
}
