#!/usr/bin/env node
/**
 * Synthesizer for genuine HelloWorld.dex bytecode executable.
 * Generates a valid Dalvik Executable with LHelloWorld; class, <init>()V,
 * and static main([Ljava/lang/String;)V method.
 */

import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const outDir = __dirname;

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

export function generateHelloWorldDex() {
    const strings = [
        "",
        "<init>",
        "Hello from ART inside v86",
        "LHelloWorld;",
        "Ljava/io/PrintStream;",
        "Ljava/lang/Object;",
        "Ljava/lang/String;",
        "Ljava/lang/System;",
        "V",
        "[Ljava/lang/String;",
        "main",
        "out",
        "println"
    ];

    const types = [
        "LHelloWorld;",
        "Ljava/io/PrintStream;",
        "Ljava/lang/Object;",
        "Ljava/lang/String;",
        "Ljava/lang/System;",
        "V",
        "[Ljava/lang/String;"
    ];

    // Protos:
    // 0: ()V (return V, no params)
    // 1: ([Ljava/lang/String;)V (return V, params [Ljava/lang/String;)
    // 2: (Ljava/lang/String;)V (return V, params Ljava/lang/String;)

    const HEADER_SIZE = 0x70; // 112 bytes
    const stringIdsOff = HEADER_SIZE;
    const stringIdsSize = strings.length;
    const typeIdsOff = stringIdsOff + (stringIdsSize * 4);
    const typeIdsSize = types.length;
    const protoIdsOff = typeIdsOff + (typeIdsSize * 4);
    const protoIdsSize = 3;
    const fieldIdsOff = protoIdsOff + (protoIdsSize * 12);
    const fieldIdsSize = 1;
    const methodIdsOff = fieldIdsOff + (fieldIdsSize * 8);
    const methodIdsSize = 3;
    const classDefsOff = methodIdsOff + (methodIdsSize * 8);
    const classDefsSize = 1;

    // Type lists for proto params
    const typeListsOff = classDefsOff + (classDefsSize * 32);
    // proto 1 params: [Ljava/lang/String;] -> size 1, type_idx 6
    // proto 2 params: Ljava/lang/String; -> size 1, type_idx 3
    const typeList1Off = typeListsOff;
    const typeList2Off = typeList1Off + 8; // 4 bytes size + 2 bytes type + 2 pad

    // Code items
    // code 0 (<init>): return-void (0x000e)
    const codeItem0Off = typeList2Off + 8;
    const codeItem0Size = 16 + 4; // 16 header + 4 insns bytes = 20
    const codeItem1Off = codeItem0Off + codeItem0Size;
    const codeItem1Size = 16 + 16; // 16 header + 16 insns bytes = 32

    // Class data item
    const classDataOff = codeItem1Off + codeItem1Size;
    const classDataSize = 32;

    // Map list
    const mapListOff = classDataOff + classDataSize;
    const mapItemCount = 11;
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

    // 1. Magic
    dex.write('dex\n035\0', 0, 8, 'ascii');

    // 2. Header
    dex.writeUInt32LE(totalFileSize, 0x20);      // file_size
    dex.writeUInt32LE(HEADER_SIZE, 0x24);        // header_size
    dex.writeUInt32LE(0x12345678, 0x28);        // endian_tag
    dex.writeUInt32LE(0, 0x2C);                 // link_size
    dex.writeUInt32LE(0, 0x30);                 // link_off
    dex.writeUInt32LE(mapListOff, 0x34);        // map_off
    dex.writeUInt32LE(stringIdsSize, 0x38);     // string_ids_size
    dex.writeUInt32LE(stringIdsOff, 0x3C);      // string_ids_off
    dex.writeUInt32LE(typeIdsSize, 0x40);       // type_ids_size
    dex.writeUInt32LE(typeIdsOff, 0x44);        // type_ids_off
    dex.writeUInt32LE(protoIdsSize, 0x48);      // proto_ids_size
    dex.writeUInt32LE(protoIdsOff, 0x4C);       // proto_ids_off
    dex.writeUInt32LE(fieldIdsSize, 0x50);      // field_ids_size
    dex.writeUInt32LE(fieldIdsOff, 0x54);       // field_ids_off
    dex.writeUInt32LE(methodIdsSize, 0x58);     // method_ids_size
    dex.writeUInt32LE(methodIdsOff, 0x5C);      // method_ids_off
    dex.writeUInt32LE(classDefsSize, 0x60);      // class_defs_size
    dex.writeUInt32LE(classDefsOff, 0x64);       // class_defs_off
    dex.writeUInt32LE(totalFileSize - mapListOff, 0x68); // data_size
    dex.writeUInt32LE(mapListOff, 0x6C);        // data_off

    // 3. String IDs
    for (let i = 0; i < stringIdsSize; i++) {
        dex.writeUInt32LE(stringDataOffsets[i], stringIdsOff + (i * 4));
    }

    // 4. Type IDs
    for (let i = 0; i < typeIdsSize; i++) {
        const stringIdx = strings.indexOf(types[i]);
        dex.writeUInt32LE(stringIdx, typeIdsOff + (i * 4));
    }

    // 5. Proto IDs
    // proto 0: ()V -> shorty_idx "V" (strings[8]), return_type_idx "V" (types[5]), parameters_off 0
    dex.writeUInt32LE(strings.indexOf("V"), protoIdsOff + 0);
    dex.writeUInt32LE(types.indexOf("V"), protoIdsOff + 4);
    dex.writeUInt32LE(0, protoIdsOff + 8);

    // proto 1: ([Ljava/lang/String;)V -> shorty_idx "V" (strings[8]), return_type_idx "V" (types[5]), params typeList1Off
    dex.writeUInt32LE(strings.indexOf("V"), protoIdsOff + 12);
    dex.writeUInt32LE(types.indexOf("V"), protoIdsOff + 16);
    dex.writeUInt32LE(typeList1Off, protoIdsOff + 20);

    // proto 2: (Ljava/lang/String;)V -> shorty_idx "V" (strings[8]), return_type_idx "V" (types[5]), params typeList2Off
    dex.writeUInt32LE(strings.indexOf("V"), protoIdsOff + 24);
    dex.writeUInt32LE(types.indexOf("V"), protoIdsOff + 28);
    dex.writeUInt32LE(typeList2Off, protoIdsOff + 32);

    // TypeLists
    // typeList1: size 1, type_idx 6 ([Ljava/lang/String;)
    dex.writeUInt32LE(1, typeList1Off);
    dex.writeUInt16LE(types.indexOf("[Ljava/lang/String;"), typeList1Off + 4);
    dex.writeUInt16LE(0, typeList1Off + 6); // pad

    // typeList2: size 1, type_idx 3 (Ljava/lang/String;)
    dex.writeUInt32LE(1, typeList2Off);
    dex.writeUInt16LE(types.indexOf("Ljava/lang/String;"), typeList2Off + 4);
    dex.writeUInt16LE(0, typeList2Off + 6); // pad

    // 6. Field IDs
    // field 0: Ljava/lang/System; (type 4), Ljava/io/PrintStream; (type 1), name "out" (string 11)
    dex.writeUInt16LE(types.indexOf("Ljava/lang/System;"), fieldIdsOff + 0);
    dex.writeUInt16LE(types.indexOf("Ljava/io/PrintStream;"), fieldIdsOff + 2);
    dex.writeUInt32LE(strings.indexOf("out"), fieldIdsOff + 4);

    // 7. Method IDs
    // method 0: LHelloWorld; (type 0), proto 0, name "<init>" (string 1)
    dex.writeUInt16LE(types.indexOf("LHelloWorld;"), methodIdsOff + 0);
    dex.writeUInt16LE(0, methodIdsOff + 2);
    dex.writeUInt32LE(strings.indexOf("<init>"), methodIdsOff + 4);

    // method 1: LHelloWorld; (type 0), proto 1, name "main" (string 10)
    dex.writeUInt16LE(types.indexOf("LHelloWorld;"), methodIdsOff + 8);
    dex.writeUInt16LE(1, methodIdsOff + 10);
    dex.writeUInt32LE(strings.indexOf("main"), methodIdsOff + 12);

    // method 2: Ljava/io/PrintStream; (type 1), proto 2, name "println" (string 12)
    dex.writeUInt16LE(types.indexOf("Ljava/io/PrintStream;"), methodIdsOff + 16);
    dex.writeUInt16LE(2, methodIdsOff + 18);
    dex.writeUInt32LE(strings.indexOf("println"), methodIdsOff + 20);

    // 8. Class Def 0: LHelloWorld;
    dex.writeUInt32LE(types.indexOf("LHelloWorld;"), classDefsOff + 0x00);      // class_idx (0)
    dex.writeUInt32LE(0x0001, classDefsOff + 0x04); // access_flags (ACC_PUBLIC)
    dex.writeUInt32LE(types.indexOf("Ljava/lang/Object;"), classDefsOff + 0x08); // superclass_idx (Ljava/lang/Object; = 2)
    dex.writeUInt32LE(0, classDefsOff + 0x0C);      // interfaces_off
    dex.writeUInt32LE(0xFFFFFFFF >>> 0, classDefsOff + 0x10); // source_file_idx
    dex.writeUInt32LE(0, classDefsOff + 0x14);      // annotations_off
    dex.writeUInt32LE(classDataOff, classDefsOff + 0x18); // class_data_off
    dex.writeUInt32LE(0, classDefsOff + 0x1C);      // static_values_off

    // 9. Code Items
    // code 0 (<init>): registers_size: 1, ins_size: 1, outs_size: 0, tries_size: 0, debug_info_off: 0, insns_size: 1
    dex.writeUInt16LE(1, codeItem0Off + 0);
    dex.writeUInt16LE(1, codeItem0Off + 2);
    dex.writeUInt16LE(0, codeItem0Off + 4);
    dex.writeUInt16LE(0, codeItem0Off + 6);
    dex.writeUInt32LE(0, codeItem0Off + 8);
    dex.writeUInt32LE(1, codeItem0Off + 12);
    dex.writeUInt16LE(0x000e, codeItem0Off + 16); // return-void
    dex.writeUInt16LE(0x0000, codeItem0Off + 18); // padding

    // code 1 (main): registers_size: 3, ins_size: 1, outs_size: 2, tries_size: 0, debug_info_off: 0, insns_size: 8
    dex.writeUInt16LE(3, codeItem1Off + 0);
    dex.writeUInt16LE(1, codeItem1Off + 2);
    dex.writeUInt16LE(2, codeItem1Off + 4);
    dex.writeUInt16LE(0, codeItem1Off + 6);
    dex.writeUInt32LE(0, codeItem1Off + 8);
    dex.writeUInt32LE(8, codeItem1Off + 12);
    // insns:
    // const-string v0, string 2 ("Hello from ART inside v86") -> 0x001a, 0x0002
    dex.writeUInt16LE(0x001a, codeItem1Off + 16);
    dex.writeUInt16LE(strings.indexOf("Hello from ART inside v86"), codeItem1Off + 18);
    // sget-object v1, field 0 (System.out) -> 0x0162, 0x0000
    dex.writeUInt16LE(0x0162, codeItem1Off + 20);
    dex.writeUInt16LE(0x0000, codeItem1Off + 22);
    // invoke-virtual {v1, v0}, method 2 (PrintStream.println) -> 0x206e, 0x0002, 0x0010
    dex.writeUInt16LE(0x206e, codeItem1Off + 24);
    dex.writeUInt16LE(0x0002, codeItem1Off + 26);
    dex.writeUInt16LE(0x0010, codeItem1Off + 28);
    // return-void -> 0x000e
    dex.writeUInt16LE(0x000e, codeItem1Off + 30);

    // 10. Class Data Item
    let cdPos = classDataOff;
    dex.set(encodeUleb128(0), cdPos); cdPos++;
    dex.set(encodeUleb128(0), cdPos); cdPos++;
    dex.set(encodeUleb128(2), cdPos); cdPos++;
    dex.set(encodeUleb128(0), cdPos); cdPos++;

    // direct method 0: method_idx_diff (0), access_flags (0x10001 = ACC_PUBLIC | ACC_CONSTRUCTOR), code_off (codeItem0Off)
    const m0Diff = encodeUleb128(0);
    dex.set(m0Diff, cdPos); cdPos += m0Diff.length;
    const m0Flags = encodeUleb128(0x10001);
    dex.set(m0Flags, cdPos); cdPos += m0Flags.length;
    const m0Code = encodeUleb128(codeItem0Off);
    dex.set(m0Code, cdPos); cdPos += m0Code.length;

    // direct method 1: method_idx_diff (1), access_flags (0x0009 = ACC_PUBLIC | ACC_STATIC), code_off (codeItem1Off)
    const m1Diff = encodeUleb128(1);
    dex.set(m1Diff, cdPos); cdPos += m1Diff.length;
    const m1Flags = encodeUleb128(0x0009);
    dex.set(m1Flags, cdPos); cdPos += m1Flags.length;
    const m1Code = encodeUleb128(codeItem1Off);
    dex.set(m1Code, cdPos); cdPos += m1Code.length;

    // 11. Map List
    dex.writeUInt32LE(mapItemCount, mapListOff);
    const mapItems = [
        { type: 0x0000, size: 1, offset: 0 },                  // kDexTypeHeaderItem
        { type: 0x0001, size: stringIdsSize, offset: stringIdsOff }, // kDexTypeStringIdItem
        { type: 0x0002, size: typeIdsSize, offset: typeIdsOff },     // kDexTypeTypeIdItem
        { type: 0x0003, size: protoIdsSize, offset: protoIdsOff },   // kDexTypeProtoIdItem
        { type: 0x0004, size: fieldIdsSize, offset: fieldIdsOff },   // kDexTypeFieldIdItem
        { type: 0x0005, size: methodIdsSize, offset: methodIdsOff }, // kDexTypeMethodIdItem
        { type: 0x0006, size: classDefsSize, offset: classDefsOff }, // kDexTypeClassDefItem
        { type: 0x1000, size: 1, offset: mapListOff },         // kDexTypeMapList
        { type: 0x1001, size: 2, offset: typeListsOff },       // kDexTypeTypeList
        { type: 0x2001, size: 2, offset: codeItem0Off },       // kDexTypeCodeItem
        { type: 0x2002, size: stringIdsSize, offset: stringDataStartOff } // kDexTypeStringDataItem
    ];

    for (let i = 0; i < mapItems.length; i++) {
        const itemOff = mapListOff + 4 + (i * 12);
        dex.writeUInt16LE(mapItems[i].type, itemOff + 0);
        dex.writeUInt16LE(0, itemOff + 2);
        dex.writeUInt32LE(mapItems[i].size, itemOff + 4);
        dex.writeUInt32LE(mapItems[i].offset, itemOff + 8);
    }

    // 12. Write String Data Items
    for (let i = 0; i < stringIdsSize; i++) {
        stringDataBuffers[i].copy(dex, stringDataOffsets[i]);
    }

    // 13. SHA-1 Signature
    const sha1 = crypto.createHash('sha1').update(dex.subarray(32)).digest();
    sha1.copy(dex, 12);

    // 14. Adler-32 Checksum
    const checksum = adler32(dex, 12, totalFileSize - 12);
    dex.writeUInt32LE(checksum, 8);

    return dex;
}

const dexBuf = generateHelloWorldDex();
const outPath = path.join(outDir, 'HelloWorld.dex');
fs.writeFileSync(outPath, dexBuf);
console.log(`[generate_hello_world_dex] Created ${outPath} (${dexBuf.length} bytes, magic 'dex\\n035\\0')`);
