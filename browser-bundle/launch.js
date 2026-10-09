import createQemu from './assets/qemu-system-aarch64.js';

const startButton = document.querySelector('#start');
const statusElement = document.querySelector('#status');
const serialElement = document.querySelector('#serial');
const initMarker = 'Run /init as init process';
let started = false;
const romFiles = ["efi-e1000.rom", "efi-e1000e.rom", "efi-eepro100.rom", "efi-ne2k_pci.rom", "efi-pcnet.rom", "efi-rtl8139.rom", "efi-virtio.rom", "efi-vmxnet3.rom", "pxe-e1000.rom", "pxe-eepro100.rom", "pxe-ne2k_pci.rom", "pxe-pcnet.rom", "pxe-rtl8139.rom", "pxe-virtio.rom", "qboot.rom"];
let markerTail = '';

window.__INIT_SEEN = false;
window.__HARNESS_STATUS = 'idle';

function setStatus(status) {
  statusElement.textContent = status;
  statusElement.dataset.state = status;
  window.__HARNESS_STATUS = status;
}

function printSerial(chunk) {
  const text = String(chunk);
  serialElement.textContent += text;
  if (!text.endsWith('\n')) serialElement.textContent += '\n';
  serialElement.scrollTop = serialElement.scrollHeight;

  const combined = markerTail + text;
  if (!window.__INIT_SEEN && combined.includes(initMarker)) {
    window.__INIT_SEEN = true;
    setStatus('init-seen');
  }
  markerTail = combined.slice(-(initMarker.length + 2048));
}

async function fetchGuestFile(name) {
  const url = new URL(`./assets/${name}`, import.meta.url);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

startButton.addEventListener('click', async () => {
  if (started) return;
  started = true;
  startButton.disabled = true;
  setStatus('downloading guest files');

  try {
    const romPromises = romFiles.map(f => fetchGuestFile(`roms/${f}`));
    const [image, initramfs, dtb, ...romData] = await Promise.all([
      fetchGuestFile('Image'),
      fetchGuestFile('initramfs.cpio'),
      fetchGuestFile('minimal-virt-fixed.dtb'),
      ...romPromises,
    ]);

    const moduleArg = {
      arguments: [
        '-accel', 'tcg',
        '-machine', 'virt',
        '-cpu', 'cortex-a57',
        '-m', '512',
        '-kernel', '/Image',
        '-initrd', '/initramfs.cpio',
        '-dtb', '/minimal-virt-fixed.dtb',
        '-display', 'none',
        '-no-reboot',
        '-monitor', 'none',
        '-serial', 'stdio',
        '-L', '/qemu-data',
        '-append', 'console=ttyAMA0 rdinit=/init',
      ],
      print: printSerial,
      printErr: printSerial,
      locateFile(path) {
        return new URL(`./assets/${path}`, import.meta.url).href;
      },
      preRun: [() => {
        if (!moduleArg.FS) throw new Error('Emscripten FS is unavailable');
        moduleArg.FS.writeFile('/Image', image);
        moduleArg.FS.writeFile('/initramfs.cpio', initramfs);
        moduleArg.FS.writeFile('/minimal-virt-fixed.dtb', dtb);
        romFiles.forEach((f, i) => {
          moduleArg.FS.writeFile(`/${f}`, romData[i]);
          try { moduleArg.FS.mkdir('/qemu-data'); } catch(e) {}
          moduleArg.FS.writeFile(`/qemu-data/${f}`, romData[i]);
        });
      }],
      onRuntimeInitialized() {
        if (!window.__INIT_SEEN) setStatus('running');
      },
      onAbort(reason) {
        setStatus(`error: QEMU aborted (${reason})`);
      },
    };

    setStatus('starting QEMU');
    createQemu(moduleArg).then(() => {
      if (!window.__INIT_SEEN) setStatus('QEMU exited');
    }).catch((error) => {
      setStatus(`error: ${error.message}`);
    });
  } catch (error) {
    setStatus(`error: ${error.message}`);
  }
});
