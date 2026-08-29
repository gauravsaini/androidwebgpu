/**
 * Test Suite for Authentic Android 9.0 (Pie) Framework & ART Runtime Assets
 * 
 * Verifies:
 * 1. boot.art binary image layout (18MB size, ART 018 magic, image base, sections, and roots table).
 * 2. Multiarch libart.so shared objects (>=1.2MB size, ELF32/64 ET_DYN, JNI & ART C++ symbol exports).
 * 3. Framework JAR bytecode archives (core-libart.jar, ext.jar, framework.jar, services.jar) with >10,000 classes and DEX 035 wire structures.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { ApkZipReader } from '../src/apk_client_parser.js';
import { DexParser } from '../src/dex_vm.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');
const frameworkDir = path.resolve(rootDir, 'guest/initrd/system/framework');
const lib32Dir = path.resolve(rootDir, 'guest/initrd/system/lib');
const lib64Dir = path.resolve(rootDir, 'guest/initrd/system/lib64');

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`❌ FAIL: ${message}`);
        failed++;
        throw new Error(message);
    } else {
        passed++;
    }
}

console.log("===================================================================");
console.log("▶ Running Authentic Android 9.0 Framework & ART Verification Suite");
console.log("===================================================================");

// ---------------------------------------------------------------------------
// 1. Verify boot.art (18MB authentic ART 018 image)
// ---------------------------------------------------------------------------
console.log("\n1. Verifying boot.art binary image structure...");
const bootArtPath = path.join(frameworkDir, 'boot.art');
assert(fs.existsSync(bootArtPath), "boot.art must exist in system/framework");

const bootArtBuf = fs.readFileSync(bootArtPath);
assert(bootArtBuf.length >= 18000000, `boot.art size (${bootArtBuf.length} bytes) must be >= 18MB`);

const artMagic = bootArtBuf.toString('ascii', 0, 4);
const artVersion = bootArtBuf.toString('ascii', 4, 8);
assert(artMagic === 'art\n', `boot.art magic must be 'art\\n', got '${artMagic}'`);
assert(artVersion === '018\0', `boot.art version must be '018\\0', got '${artVersion}'`);

const imageBase = bootArtBuf.readUInt32LE(0x08);
const imageSize = bootArtBuf.readUInt32LE(0x0C);
assert(imageBase === 0x70000000, `Image base must be 0x70000000, got 0x${imageBase.toString(16)}`);
assert(imageSize === 18874368, `Image size must be 18874368 (18MB), got ${imageSize}`);

const rootsCount = bootArtBuf.readUInt32LE(0x1000);
assert(rootsCount >= 8, `Image roots count must be >= 8, got ${rootsCount}`);
console.log(`✔ boot.art passed: ${bootArtBuf.length} bytes, ART 018, Base 0x70000000, Roots: ${rootsCount}`);

// ---------------------------------------------------------------------------
// 2. Verify libart.so Multiarch Shared Objects (>=1.2MB, ELF32/64, Symbols)
// ---------------------------------------------------------------------------
console.log("\n2. Verifying multiarch libart.so shared objects (system/lib and system/lib64)...");

function verifyLibArtElf(filePath, is64Bit) {
    assert(fs.existsSync(filePath), `libart.so must exist at ${filePath}`);
    const elfBuf = fs.readFileSync(filePath);
    assert(elfBuf.length >= 1200000, `libart.so size (${elfBuf.length} bytes) must be >= 1.2MB`);

    // ELF Header Magic: \x7FELF
    const elfMagic = elfBuf.toString('ascii', 0, 4);
    assert(elfMagic === '\x7FELF', `ELF magic must be '\\x7FELF', got '${elfMagic}'`);

    const elfClass = elfBuf[4];
    assert(elfClass === (is64Bit ? 2 : 1), `ELF class must match target arch (${is64Bit ? '64-bit' : '32-bit'})`);

    const elfType = elfBuf.readUInt16LE(16);
    assert(elfType === 3, `ELF type must be ET_DYN (3), got ${elfType}`);

    const elfMachine = elfBuf.readUInt16LE(18);
    assert(elfMachine === (is64Bit ? 62 : 3), `ELF machine must match arch (${is64Bit ? 'EM_X86_64' : 'EM_386'})`);

    // Check exported symbols in binary string pool
    const strData = elfBuf.toString('utf8');
    const requiredSymbols = [
        "JNI_CreateJavaVM",
        "JNI_GetDefaultJavaVMInitArgs",
        "JNI_GetCreatedJavaVMs",
        "_ZN3art7Runtime9instance_E",
        "_ZN3art7Runtime6CreateEPNS_14RuntimeOptionsE",
        "_ZN3art7Runtime5StartEv",
        "_ZN3art11ClassLinker9FindClassEPNS_6ThreadEPKcPNS_6mirror11ClassLoaderE",
        "_ZN3art7DexFile10OpenMemoryEPKhjRKNSt3__112basic_stringIcNS3_11char_traitsIcEENS3_9allocatorIcEEEEjPNS_6MemMapEPKNS_10OatDexFileEPS9_",
        "_ZN3art3ArtMethod6InvokeEPNS_6ThreadEPjjPNS_6JValueEPKc"
    ];

    for (const sym of requiredSymbols) {
        assert(strData.includes(sym), `libart.so must contain exported symbol '${sym}'`);
    }

    console.log(`✔ libart.so (${is64Bit ? '64-bit' : '32-bit'}) verified: ${elfBuf.length} bytes, ET_DYN, symbols validated.`);
}

verifyLibArtElf(path.join(lib32Dir, 'libart.so'), false);
verifyLibArtElf(path.join(lib64Dir, 'libart.so'), true);

// ---------------------------------------------------------------------------
// 3. Verify Framework Bytecode JARs & DEX 035 wire layout (>10,000 classes)
// ---------------------------------------------------------------------------
console.log("\n3. Verifying Framework JAR archives (core-libart, ext, framework, services)...");

const jars = [
    { name: 'core-libart.jar', minClasses: 2500 },
    { name: 'ext.jar', minClasses: 700 },
    { name: 'framework.jar', minClasses: 5000 },
    { name: 'services.jar', minClasses: 1400 }
];

let totalFrameworkClasses = 0;

for (const jarInfo of jars) {
    const jarPath = path.join(frameworkDir, jarInfo.name);
    assert(fs.existsSync(jarPath), `${jarInfo.name} must exist in system/framework`);

    const jarBuf = fs.readFileSync(jarPath);
    assert(jarBuf.length > 50000, `${jarInfo.name} size (${jarBuf.length} bytes) must exceed 50KB`);

    // Verify ZIP header
    const zipMagic = jarBuf.readUInt32LE(0);
    assert(zipMagic === 0x04034B50, `${jarInfo.name} must be a valid ZIP archive (PK\\x03\\x04)`);

    const zipReader = new ApkZipReader(jarBuf);
    const dexBytes = zipReader.readFile('classes.dex');
    assert(dexBytes && dexBytes.length > 0, `${jarInfo.name} must contain classes.dex`);

    // Parse DEX binary layout
    const parser = new DexParser(dexBytes, `${jarInfo.name}/classes.dex`).parse();
    assert(parser.classes.size >= jarInfo.minClasses, `${jarInfo.name} class count (${parser.classes.size}) must be >= ${jarInfo.minClasses}`);

    totalFrameworkClasses += parser.classes.size;
    console.log(`✔ ${jarInfo.name}: ${jarBuf.length} bytes, ${parser.classes.size} classes, ${parser.methods.length} methods parsed.`);
}

assert(totalFrameworkClasses >= 10000, `Total authentic framework classes (${totalFrameworkClasses}) must exceed 10,000`);
console.log(`\n✔ Framework Class Catalog: ${totalFrameworkClasses} total classes loaded and verified!`);

console.log("\n===================================================================");
console.log(`🎉 ALL ${passed} AUTHENTIC FRAMEWORK & ART TESTS PASSED (0 failures)`);
console.log("===================================================================");
