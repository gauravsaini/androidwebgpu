import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { DexParser, DalvikVM } from '../../src/dex_vm.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const dexPath = path.join(__dirname, 'HelloWorld.dex');
const dexBuf = fs.readFileSync(dexPath);

console.log(`Testing HelloWorld.dex parsing and Dalvik VM execution...`);
const parser = new DexParser(dexBuf, 'HelloWorld.dex').parse();
console.log(`Parsed classes count: ${parser.classes.size}`);
console.log(`Parsed methods count: ${parser.methods.length}`);
console.log(`Parsed strings count: ${parser.strings.length}`);

const vm = new DalvikVM();
vm.loadDex(parser);

const helloClass = vm.findClass('HelloWorld');
if (!helloClass) {
    throw new Error('Failed to find HelloWorld class in DalvikVM');
}
console.log(`Found class: ${helloClass.name} (normalized: ${helloClass.normalizedName})`);
console.log(`Direct methods:`, Array.from(helloClass.directMethods.keys()));

if (!helloClass.directMethods.has('<init>')) {
    throw new Error('Missing <init> method');
}
if (!helloClass.directMethods.has('main')) {
    throw new Error('Missing main method');
}

console.log('✔ HelloWorld.dex successfully verified against DexParser and DalvikVM!');
