/**
 * Pure UI Rendering Engine for Android WebGPU UI
 */

export const PRIO_COLORS = {
    'E': '#ef4444',
    'W': '#f59e0b',
    'D': '#38bdf8',
    'V': '#94a3b8',
    'I': '#10b981'
};

export const BINDER_HANDLE_NAMES = {
    1: 'SurfaceComposer',
    2: 'InputManager',
    3: 'WindowManager',
    4: 'ActivityManager',
    5: 'PackageManager',
    10: 'BufferProducer_10',
    20: 'BufferProducer_20',
    30: 'BufferProducer_30'
};

/**
 * Creates a DOM node for a single logcat entry.
 */
export function createLogcatElement(entry) {
    const line = document.createElement('div');
    line.style.color = PRIO_COLORS[entry.priority] || '#cbd5e1';
    line.style.whiteSpace = 'pre-wrap';
    line.style.wordBreak = 'break-word';
    line.textContent = entry.formatted;
    return line;
}

/**
 * Re-renders the entire filtered logcat buffer.
 */
export function renderLogcatList(containerEl, filteredEntries, counterEl, totalCount, isAutoScrollEnabled) {
    if (!containerEl) return;
    containerEl.innerHTML = '<div class="log-header-main">--------- beginning of main</div>';
    for (const entry of filteredEntries) {
        containerEl.appendChild(createLogcatElement(entry));
    }
    if (counterEl) {
        counterEl.textContent = `${filteredEntries.length} / ${totalCount}`;
    }
    if (isAutoScrollEnabled) {
        containerEl.scrollTop = containerEl.scrollHeight;
    }
}

/**
 * Appends a single logcat entry to the active DOM container if it matches active filter.
 */
export function appendLogcatToDom(containerEl, entry, counterEl, totalCount, isAutoScrollEnabled, maxNodes = 5000) {
    if (!containerEl) return;
    containerEl.appendChild(createLogcatElement(entry));
    while (containerEl.childElementCount > maxNodes + 1) {
        containerEl.removeChild(containerEl.children[1]); // Keep header at index 0
    }
    if (counterEl) {
        const visibleCount = Math.max(0, containerEl.childElementCount - 1);
        counterEl.textContent = `${visibleCount} / ${totalCount}`;
    }
    if (isAutoScrollEnabled) {
        containerEl.scrollTop = containerEl.scrollHeight;
    }
}

/**
 * Renders a Binder transaction card in the network/binder log panel.
 */
export function renderBinderTransaction(containerEl, countEl, txCount, { handle, code, desc, status, durationMs, payloadSize }) {
    if (countEl) {
        countEl.textContent = txCount;
    }
    if (!containerEl) return;

    const card = document.createElement('div');
    card.className = 'traffic-card';
    const isOk = status === 0;
    const name = BINDER_HANDLE_NAMES[handle] || `Handle ${handle}`;

    card.innerHTML = `
        <div class="traffic-header">
            <span class="${isOk ? 'traffic-method-200' : 'traffic-method-err'}">BINDER [H${handle} ${name}]</span>
            <span class="traffic-duration">${Number(durationMs).toFixed(1)} ms</span>
        </div>
        <div class="traffic-detail">
            Opcode: ${code} • ${desc || 'AIDL Transact'} (${payloadSize} B) -&gt; Status ${status}
        </div>
    `;
    containerEl.insertBefore(card, containerEl.firstChild);
}

/**
 * Renders an app icon on the home launcher grid.
 */
export function renderAppLauncherItem(containerEl, pkg, name, icon, onLaunch) {
    if (!containerEl) return;
    let existing = containerEl.querySelector(`.app-icon-item[data-pkg="${pkg}"]`);
    if (existing) return;

    const item = document.createElement('div');
    item.className = 'app-icon-item';
    item.setAttribute('data-pkg', pkg);
    item.innerHTML = `
        <div class="app-icon-badge">${icon || '📦'}</div>
        <span class="app-icon-name">${name}</span>
    `;
    if (typeof onLaunch === 'function') {
        item.addEventListener('click', () => onLaunch(pkg));
    }
    containerEl.appendChild(item);
}

/**
 * Renders items in the bottom dock.
 */
export function renderDockItems(containerEl, dockApps, onLaunch) {
    if (!containerEl) return;
    containerEl.innerHTML = '';
    dockApps.forEach(app => {
        const item = document.createElement('div');
        item.className = 'app-icon-item';
        item.setAttribute('data-pkg', app.pkg);
        item.innerHTML = `
            <div class="app-icon-badge app-icon-badge-dock">${app.icon}</div>
        `;
        if (typeof onLaunch === 'function') {
            item.addEventListener('click', () => onLaunch(app.pkg));
        }
        containerEl.appendChild(item);
    });
}

/**
 * Shows an authentic Android toast notification.
 */
let toastTimeout = null;
export function showToast(toastEl, toastTextEl, message, durationMs = 3000) {
    if (!toastEl || !toastTextEl) return;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTextEl.textContent = message;
    toastEl.classList.add('show');
    toastTimeout = setTimeout(() => {
        toastEl.classList.remove('show');
        toastTimeout = null;
    }, durationMs);
}

/**
 * Updates header and home screen clocks.
 */
export function updateClock(clockHeaderEl, homeClockEl, homeDateEl) {
    const now = new Date();
    const timeStr = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const dateStr = now.toLocaleDateString([], { weekday: 'long', month: 'short', day: 'numeric' });
    if (clockHeaderEl) clockHeaderEl.textContent = timeStr;
    if (homeClockEl) homeClockEl.textContent = timeStr;
    if (homeDateEl) homeDateEl.textContent = dateStr;
}

/**
 * Updates FPS and GPU performance telemetry.
 */
export function updateMetrics({ fpsPillEl, statFpsEl, canvasHudFpsEl, fps, statGpuTimeEl, canvasHudGpuEl, gpuTime }) {
    if (fps !== undefined) {
        const fpsFormatted = typeof fps === 'number' ? fps.toFixed(1) : fps;
        if (fpsPillEl) fpsPillEl.textContent = `${fpsFormatted} FPS`;
        if (statFpsEl) statFpsEl.textContent = fpsFormatted;
        if (canvasHudFpsEl) canvasHudFpsEl.textContent = `${fpsFormatted} FPS`;
    }
    if (gpuTime !== undefined) {
        const gpuFormatted = typeof gpuTime === 'number' ? `${gpuTime.toFixed(1)} ms` : gpuTime;
        if (statGpuTimeEl) statGpuTimeEl.textContent = gpuFormatted;
        if (canvasHudGpuEl) canvasHudGpuEl.textContent = `${gpuFormatted} GPU`;
    }
}
