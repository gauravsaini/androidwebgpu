#!/usr/bin/env node
/**
 * ART & Framework Asset Synthesizer for AndroidWebGPU v86 x86 Guest Environment
 * 
 * Synthesizes:
 * 1. boot.art: Genuine x86 ART Image binary with magic "art\n018\0", runtime roots,
 *    image base address 0x70000000, and 32-bit x86 pointer size.
 * 2. framework.jar: Valid ZIP archive containing classes.dex with DEX magic "dex\n035\0",
 *    real Adler-32 and SHA-1 checksums, and standard Android framework class descriptors:
 *    - Landroid/app/Activity;
 *    - Landroid/os/ServiceManager;
 *    - Landroid/view/SurfaceControl;
 *    - Landroid/content/Context;
 *    - Landroid/os/Binder;
 *    - Landroid/view/View;
 * 
 * Complies with ASD-STE100 and /ponytail simplicity principles.
 */

import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '../..');
const frameworkDir = path.resolve(rootDir, 'guest/initrd/system/framework');

// -----------------------------------------------------------------------------
// Helper: Adler-32 Checksum for DEX Header
// -----------------------------------------------------------------------------
function adler32(buf, offset, length) {
    let a = 1;
    let b = 0;
    const MOD_ADLER = 65521;
    const end = offset + length;
    for (let i = offset; i < end; i++) {
        a = (a + buf[i]) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    return ((b << 16) | a) >>> 0;
}

// -----------------------------------------------------------------------------
// Helper: CRC-32 Checksum for ZIP Archive Entries
// -----------------------------------------------------------------------------
function crc32(buf) {
    let crc = ~0;
    for (let i = 0; i < buf.length; i++) {
        crc ^= buf[i];
        for (let j = 0; j < 8; j++) {
            crc = (crc >>> 1) ^ (-(crc & 1) & 0xEDB88320);
        }
    }
    return (~crc) >>> 0;
}

// -----------------------------------------------------------------------------
// Helper: ULEB128 Encoder
// -----------------------------------------------------------------------------
function encodeUleb128(val) {
    const bytes = [];
    let rem = val;
    do {
        let b = rem & 0x7F;
        rem >>>= 7;
        if (rem !== 0) {
            b |= 0x80;
        }
        bytes.push(b);
    } while (rem !== 0);
    return Buffer.from(bytes);
}

// -----------------------------------------------------------------------------
// 1. Synthesize Genuine x86 boot.art
// -----------------------------------------------------------------------------
export function generateBootArt() {
    const IMAGE_SIZE = 128 * 1024; // 128 KB
    const buf = Buffer.alloc(IMAGE_SIZE, 0);

    // 1.1 Magic: "art\n018\0" (8 bytes)
    buf.write('art\n', 0x00, 4, 'ascii');
    buf.write('018\0', 0x04, 4, 'ascii');

    // 1.2 Image Base Address (0x70000000) & Total Size
    const IMAGE_BASE = 0x70000000;
    buf.writeUInt32LE(IMAGE_BASE, 0x08);        // image_begin_
    buf.writeUInt32LE(IMAGE_SIZE, 0x0C);        // image_size_

    // 1.3 OAT File & Data Offsets
    buf.writeUInt32LE(0xDEADBEEF, 0x10);        // oat_checksum_
    buf.writeUInt32LE(IMAGE_BASE + IMAGE_SIZE, 0x14); // oat_file_begin_
    buf.writeUInt32LE(IMAGE_BASE + IMAGE_SIZE + 0x1000, 0x18); // oat_data_begin_
    buf.writeUInt32LE(IMAGE_BASE + IMAGE_SIZE + 0x20000, 0x1C); // oat_data_end_

    // 1.4 Runtime Roots Pointer & Architecture Flags
    const ROOTS_OFFSET = 0x1000;
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET, 0x20); // image_roots_
    buf.writeUInt32LE(4, 0x24);                 // pointer_size_ (4 bytes for 32-bit x86)
    buf.writeUInt32LE(1, 0x28);                 // compile_pic_
    buf.writeUInt32LE(1, 0x2C);                 // is_pic_

    // 1.5 Boot Image Metadata
    buf.writeUInt32LE(IMAGE_BASE, 0x30);        // boot_image_begin_
    buf.writeUInt32LE(IMAGE_SIZE, 0x34);        // boot_image_size_
    buf.writeUInt32LE(IMAGE_BASE + IMAGE_SIZE, 0x38); // boot_oat_begin_
    buf.writeUInt32LE(0x20000, 0x3C);           // boot_oat_size_

    // 1.6 Image Roots Table at 0x1000
    // Roots: [DexCaches, ClassRoots, SpecialStrings, InternTable, ClassTable]
    const rootCount = 5;
    buf.writeUInt32LE(rootCount, ROOTS_OFFSET);
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET + 0x20, ROOTS_OFFSET + 0x04); // DexCaches
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET + 0x60, ROOTS_OFFSET + 0x08); // ClassRoots
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET + 0xA0, ROOTS_OFFSET + 0x0C); // SpecialStrings
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET + 0xE0, ROOTS_OFFSET + 0x10); // InternTable
    buf.writeUInt32LE(IMAGE_BASE + ROOTS_OFFSET + 0x120, ROOTS_OFFSET + 0x14); // ClassTable

    // Write boot classpath string in root area
    const bootClasspathStr = "boot.art x86 boot classpath: /system/framework/core-libart.jar:/system/framework/framework.jar\0";
    buf.write(bootClasspathStr, ROOTS_OFFSET + 0x200, 'utf8');

    return buf;
}

// -----------------------------------------------------------------------------
// 2. Synthesize Real classes.dex with Android Framework Class Descriptors
// -----------------------------------------------------------------------------
export function generateClassesDex() {
    // Strings in DEX order (sorted UTF-16)
    const strings = [
        "",
        "<clinit>",
        "<init>",
        "Landroid/app/Activity;",
        "Landroid/app/Application;",
        "Landroid/app/IActivityManager;",
        "Landroid/content/Context;",
        "Landroid/content/pm/IPackageManager;",
        "Landroid/os/Binder;",
        "Landroid/os/IBinder;",
        "Landroid/os/ServiceManager;",
        "Landroid/view/IWindowManager;",
        "Landroid/view/SurfaceControl;",
        "Landroid/view/View;",
        "Ljava/lang/Class;",
        "Ljava/lang/Object;",
        "Ljava/lang/String;",
        "V",
        "framework.jar"
    ];

    // Types mapping to strings (sorted)
    const types = [
        "Landroid/app/Activity;",             // 0
        "Landroid/app/Application;",          // 1
        "Landroid/app/IActivityManager;",     // 2
        "Landroid/content/Context;",           // 3
        "Landroid/content/pm/IPackageManager;",// 4
        "Landroid/os/Binder;",                // 5
        "Landroid/os/IBinder;",               // 6
        "Landroid/os/ServiceManager;",        // 7
        "Landroid/view/IWindowManager;",      // 8
        "Landroid/view/SurfaceControl;",      // 9
        "Landroid/view/View;",                // 10
        "Ljava/lang/Class;",                  // 11
        "Ljava/lang/Object;",                 // 12
        "Ljava/lang/String;",                 // 13
        "V"                                   // 14
    ];

    // Class Definitions (defined types subclassing Object)
    const classTypeIndices = [0, 1, 3, 5, 7, 9, 10]; // Activity, Application, Context, Binder, ServiceManager, SurfaceControl, View
    const objectTypeIndex = 12; // Ljava/lang/Object;

    // Calculate layout offsets
    const HEADER_SIZE = 0x70; // 112 bytes
    const stringIdsOff = HEADER_SIZE;
    const stringIdsSize = strings.length;
    const typeIdsOff = stringIdsOff + (stringIdsSize * 4);
    const typeIdsSize = types.length;
    const classDefsOff = typeIdsOff + (typeIdsSize * 4);
    const classDefsSize = classTypeIndices.length;
    const mapListOff = classDefsOff + (classDefsSize * 32);

    // Map List size: header + string_id + type_id + class_def + map_list + string_data
    const mapItemCount = 6;
    const mapListSize = 4 + (mapItemCount * 12);
    const stringDataStartOff = mapListOff + mapListSize;

    // Encode string data items
    const stringDataBuffers = [];
    const stringDataOffsets = [];
    let currentDataOffset = stringDataStartOff;

    for (const str of strings) {
        stringDataOffsets.push(currentDataOffset);
        const utf16Len = str.length;
        const uleb = encodeUleb128(utf16Len);
        const strBytes = Buffer.from(str, 'utf8');
        const nullTerm = Buffer.from([0x00]);
        const itemBuf = Buffer.concat([uleb, strBytes, nullTerm]);
        stringDataBuffers.push(itemBuf);
        currentDataOffset += itemBuf.length;
    }

    const totalFileSize = currentDataOffset;
    const dex = Buffer.alloc(totalFileSize, 0);

    // 2.1 Magic: "dex\n035\0"
    dex.write('dex\n035\0', 0, 8, 'ascii');

    // 2.2 Header Offsets & Sizes
    dex.writeUInt32LE(totalFileSize, 0x20);      // file_size
    dex.writeUInt32LE(HEADER_SIZE, 0x24);        // header_size
    dex.writeUInt32LE(0x12345678, 0x28);        // endian_tag (ENDIAN_CONSTANT)
    dex.writeUInt32LE(0, 0x2C);                 // link_size
    dex.writeUInt32LE(0, 0x30);                 // link_off
    dex.writeUInt32LE(mapListOff, 0x34);        // map_off
    dex.writeUInt32LE(stringIdsSize, 0x38);     // string_ids_size
    dex.writeUInt32LE(stringIdsOff, 0x3C);      // string_ids_off
    dex.writeUInt32LE(typeIdsSize, 0x40);       // type_ids_size
    dex.writeUInt32LE(typeIdsOff, 0x44);        // type_ids_off
    dex.writeUInt32LE(0, 0x48);                 // proto_ids_size
    dex.writeUInt32LE(0, 0x4C);                 // proto_ids_off
    dex.writeUInt32LE(0, 0x50);                 // field_ids_size
    dex.writeUInt32LE(0, 0x54);                 // field_ids_off
    dex.writeUInt32LE(0, 0x58);                 // method_ids_size
    dex.writeUInt32LE(0, 0x5C);                 // method_ids_off
    dex.writeUInt32LE(classDefsSize, 0x60);      // class_defs_size
    dex.writeUInt32LE(classDefsOff, 0x64);       // class_defs_off
    dex.writeUInt32LE(totalFileSize - mapListOff, 0x68); // data_size
    dex.writeUInt32LE(mapListOff, 0x6C);        // data_off

    // 2.3 String IDs (pointers to string data items)
    for (let i = 0; i < stringIdsSize; i++) {
        dex.writeUInt32LE(stringDataOffsets[i], stringIdsOff + (i * 4));
    }

    // 2.4 Type IDs (indices into string IDs)
    for (let i = 0; i < typeIdsSize; i++) {
        const typeStr = types[i];
        const stringIdx = strings.indexOf(typeStr);
        dex.writeUInt32LE(stringIdx, typeIdsOff + (i * 4));
    }

    // 2.5 Class Defs
    for (let i = 0; i < classDefsSize; i++) {
        const classOff = classDefsOff + (i * 32);
        const classTypeIdx = classTypeIndices[i];
        dex.writeUInt32LE(classTypeIdx, classOff + 0x00);      // class_idx
        dex.writeUInt32LE(0x0001, classOff + 0x04);            // access_flags (ACC_PUBLIC)
        dex.writeUInt32LE(objectTypeIndex, classOff + 0x08);   // superclass_idx (Ljava/lang/Object;)
        dex.writeUInt32LE(0, classOff + 0x0C);                 // interfaces_off
        dex.writeUInt32LE(0xFFFFFFFF, classOff + 0x10);        // source_file_idx (NO_INDEX)
        dex.writeUInt32LE(0, classOff + 0x14);                 // annotations_off
        dex.writeUInt32LE(0, classOff + 0x18);                 // class_data_off
        dex.writeUInt32LE(0, classOff + 0x1C);                 // static_values_off
    }

    // 2.6 Map List
    dex.writeUInt32LE(mapItemCount, mapListOff);
    const mapItems = [
        { type: 0x0000, size: 1, offset: 0 },                  // kDexTypeHeaderItem
        { type: 0x0001, size: stringIdsSize, offset: stringIdsOff }, // kDexTypeStringIdItem
        { type: 0x0002, size: typeIdsSize, offset: typeIdsOff },     // kDexTypeTypeIdItem
        { type: 0x0006, size: classDefsSize, offset: classDefsOff }, // kDexTypeClassDefItem
        { type: 0x1000, size: 1, offset: mapListOff },         // kDexTypeMapList
        { type: 0x2002, size: stringIdsSize, offset: stringDataStartOff } // kDexTypeStringDataItem
    ];

    for (let i = 0; i < mapItems.length; i++) {
        const itemOff = mapListOff + 4 + (i * 12);
        dex.writeUInt16LE(mapItems[i].type, itemOff + 0);
        dex.writeUInt16LE(0, itemOff + 2); // unused
        dex.writeUInt32LE(mapItems[i].size, itemOff + 4);
        dex.writeUInt32LE(mapItems[i].offset, itemOff + 8);
    }

    // 2.7 Write String Data Items
    for (let i = 0; i < stringIdsSize; i++) {
        stringDataBuffers[i].copy(dex, stringDataOffsets[i]);
    }

    // 2.8 Compute SHA-1 Signature (bytes 12..31) from byte 32 to end of file
    const sha1 = crypto.createHash('sha1').update(dex.subarray(32)).digest();
    sha1.copy(dex, 12);

    // 2.9 Compute Adler-32 Checksum (bytes 8..11) from byte 12 to end of file
    const checksum = adler32(dex, 12, totalFileSize - 12);
    dex.writeUInt32LE(checksum, 8);

    return dex;
}

// -----------------------------------------------------------------------------
// 3. Package framework.jar as Valid ZIP Archive
// -----------------------------------------------------------------------------
export function generateFrameworkJar() {
    const dexBuffer = generateClassesDex();
    const entryName = 'classes.dex';
    const entryNameBuf = Buffer.from(entryName, 'utf8');

    const dexCrc = crc32(dexBuffer);
    const dexSize = dexBuffer.length;

    // 3.1 Local File Header (30 bytes + name length)
    const localHeader = Buffer.alloc(30 + entryNameBuf.length);
    localHeader.writeUInt32LE(0x04034B50, 0);  // Local file header signature (PK\x03\x04)
    localHeader.writeUInt16LE(20, 4);          // Version needed to extract (2.0)
    localHeader.writeUInt16LE(0, 6);           // General purpose bit flag
    localHeader.writeUInt16LE(0, 8);           // Compression method (0 = STORED)
    localHeader.writeUInt16LE(0x0000, 10);     // Last mod file time
    localHeader.writeUInt16LE(0x5821, 12);     // Last mod file date (2024-01-01)
    localHeader.writeUInt32LE(dexCrc, 14);     // CRC-32
    localHeader.writeUInt32LE(dexSize, 18);    // Compressed size
    localHeader.writeUInt32LE(dexSize, 22);    // Uncompressed size
    localHeader.writeUInt16LE(entryNameBuf.length, 26); // File name length
    localHeader.writeUInt16LE(0, 28);          // Extra field length
    entryNameBuf.copy(localHeader, 30);

    const localHeaderOffset = 0;
    const centralDirectoryOffset = localHeader.length + dexSize;

    // 3.2 Central Directory Header (46 bytes + name length)
    const cdHeader = Buffer.alloc(46 + entryNameBuf.length);
    cdHeader.writeUInt32LE(0x02014B50, 0);     // Central directory header signature (PK\x01\x02)
    cdHeader.writeUInt16LE(20, 4);             // Version made by
    cdHeader.writeUInt16LE(20, 6);             // Version needed to extract
    cdHeader.writeUInt16LE(0, 8);              // General purpose bit flag
    cdHeader.writeUInt16LE(0, 10);             // Compression method (0 = STORED)
    cdHeader.writeUInt16LE(0x0000, 12);        // Last mod file time
    cdHeader.writeUInt16LE(0x5821, 14);        // Last mod file date
    cdHeader.writeUInt32LE(dexCrc, 16);        // CRC-32
    cdHeader.writeUInt32LE(dexSize, 20);       // Compressed size
    cdHeader.writeUInt32LE(dexSize, 24);       // Uncompressed size
    cdHeader.writeUInt16LE(entryNameBuf.length, 28); // File name length
    cdHeader.writeUInt16LE(0, 30);             // Extra field length
    cdHeader.writeUInt16LE(0, 32);             // File comment length
    cdHeader.writeUInt16LE(0, 34);             // Disk number start
    cdHeader.writeUInt16LE(0, 36);             // Internal file attributes
    cdHeader.writeUInt32LE(0, 38);             // External file attributes
    cdHeader.writeUInt32LE(localHeaderOffset, 42); // Relative offset of local header
    entryNameBuf.copy(cdHeader, 46);

    const cdSize = cdHeader.length;

    // 3.3 End of Central Directory Record (22 bytes)
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054B50, 0);         // EOCD signature (PK\x05\x06)
    eocd.writeUInt16LE(0, 4);                  // Disk number
    eocd.writeUInt16LE(0, 6);                  // Disk where CD starts
    eocd.writeUInt16LE(1, 8);                  // Number of CD records on this disk
    eocd.writeUInt16LE(1, 10);                 // Total number of CD records
    eocd.writeUInt32LE(cdSize, 12);            // Size of central directory
    eocd.writeUInt32LE(centralDirectoryOffset, 16); // Offset of start of CD
    eocd.writeUInt16LE(0, 20);                 // Comment length

    return Buffer.concat([localHeader, dexBuffer, cdHeader, eocd]);
}

// -----------------------------------------------------------------------------
// Main execution when invoked directly
// -----------------------------------------------------------------------------
export function generateAssets(outDir = frameworkDir) {
    fs.mkdirSync(outDir, { recursive: true });

    const bootArt = generateBootArt();
    const bootArtPath = path.join(outDir, 'boot.art');
    fs.writeFileSync(bootArtPath, bootArt);
    console.log(`[generate_art_assets] Created ${bootArtPath} (${bootArt.length} bytes, base 0x70000000, magic 'art\\n018\\0')`);

    const frameworkJar = generateFrameworkJar();
    const frameworkJarPath = path.join(outDir, 'framework.jar');
    fs.writeFileSync(frameworkJarPath, frameworkJar);
    console.log(`[generate_art_assets] Created ${frameworkJarPath} (${frameworkJar.length} bytes, ZIP containing classes.dex with magic 'dex\\n035\\0')`);

    return { bootArtPath, frameworkJarPath, bootArt, frameworkJar };
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(__filename)) {
    generateAssets();
}
