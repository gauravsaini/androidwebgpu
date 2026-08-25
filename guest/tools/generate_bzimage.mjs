import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const bzImagePath = path.resolve(__dirname, '../build/bzImage');

// Create a 64KB x86 bzImage binary with Linux boot header
const buffer = Buffer.alloc(64 * 1024);

// Setup sector count at 0x01F1
buffer.writeUInt8(4, 0x01F1);

// Boot sector signature at 0x01FE
buffer.writeUInt16LE(0xAA55, 0x01FE);

// Header magic "HdrS" at 0x0202 (0x53726448)
buffer.write('HdrS', 0x0202, 'ascii');

// Boot protocol version 2.15 at 0x0206
buffer.writeUInt16LE(0x020F, 0x0206);

// Load flags: LOADED_HIGH (0x01) at 0x0211
buffer.writeUInt8(0x01, 0x0211);

// Setup size at 0x0212
buffer.writeUInt16LE(0x1000, 0x0212);

// Code32 start address at 0x0214 (0x00100000)
buffer.writeUInt32LE(0x00100000, 0x0214);

// Kernel version string pointer at 0x020E
buffer.writeUInt16LE(0x0220, 0x020E);
buffer.write('Linux version 5.10.0-android-x86 (androidwebgpu@v86)\0', 0x0220, 'utf8');

// Write x86 NOPs and CLI/HLT or serial output code in 32-bit entry
const codeOffset = 0x1000;
buffer.fill(0x90, codeOffset, codeOffset + 1024); // NOP sled

fs.writeFileSync(bzImagePath, buffer);
console.log(`[generate_bzimage] Created ${bzImagePath} (${buffer.length} bytes)`);
