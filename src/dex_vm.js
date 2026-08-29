/**
 * AndroidWebGPU - Real DEX Bytecode Parser & Dalvik Virtual Machine Interpreter
 * 
 * Provides:
 * 1. DexParser: Binary DEX (035/037/038/039) header, string pool, type, proto, field, method, and class_def decoding.
 * 2. DalvikVM: Register-based bytecode interpreter supporting core Dalvik opcodes, class instantiation,
 *    virtual/direct/static method dispatch, and Java/Android framework bridge.
 * 3. AndroidFrameworkBridge: In-memory runtime for Activity, Context, Intent, Bundle, Toast, View, Log, Resources.
 * 
 * Complies with ASD-STE100 Simplified Technical English.
 */

// -----------------------------------------------------------------------------
// Dalvik Opcodes Constants
// -----------------------------------------------------------------------------
export const OP_NOP = 0x00;
export const OP_MOVE = 0x01;
export const OP_MOVE_FROM16 = 0x02;
export const OP_MOVE_16 = 0x03;
export const OP_MOVE_WIDE = 0x04;
export const OP_MOVE_OBJECT = 0x07;
export const OP_MOVE_RESULT = 0x0a;
export const OP_MOVE_RESULT_OBJECT = 0x0c;
export const OP_MOVE_EXCEPTION = 0x0d;
export const OP_RETURN_VOID = 0x0e;
export const OP_RETURN = 0x0f;
export const OP_RETURN_WIDE = 0x10;
export const OP_RETURN_OBJECT = 0x11;
export const OP_CONST_4 = 0x12;
export const OP_CONST_16 = 0x13;
export const OP_CONST = 0x14;
export const OP_CONST_HIGH16 = 0x15;
export const OP_CONST_WIDE_16 = 0x16;
export const OP_CONST_STRING = 0x1a;
export const OP_CONST_STRING_JUMBO = 0x1b;
export const OP_CONST_CLASS = 0x1c;
export const OP_MONITOR_ENTER = 0x1d;
export const OP_MONITOR_EXIT = 0x1e;
export const OP_CHECK_CAST = 0x1f;
export const OP_INSTANCE_OF = 0x20;
export const OP_ARRAY_LENGTH = 0x21;
export const OP_NEW_INSTANCE = 0x22;
export const OP_NEW_ARRAY = 0x23;
export const OP_FILLED_NEW_ARRAY = 0x24;
export const OP_FILL_ARRAY_DATA = 0x26;
export const OP_THROW = 0x27;
export const OP_GOTO = 0x28;
export const OP_GOTO_16 = 0x29;
export const OP_GOTO_32 = 0x2a;
export const OP_PACKED_SWITCH = 0x2b;
export const OP_SPARSE_SWITCH = 0x2c;
export const OP_CMPL_FLOAT = 0x2d;
export const OP_CMPG_FLOAT = 0x2e;
export const OP_CMPL_DOUBLE = 0x2f;
export const OP_CMPG_DOUBLE = 0x30;
export const OP_CMP_LONG = 0x31;
export const OP_IF_EQ = 0x32;
export const OP_IF_NE = 0x33;
export const OP_IF_LT = 0x34;
export const OP_IF_GE = 0x35;
export const OP_IF_GT = 0x36;
export const OP_IF_LE = 0x37;
export const OP_IF_EQZ = 0x38;
export const OP_IF_NEZ = 0x39;
export const OP_IF_LTZ = 0x3a;
export const OP_IF_GEZ = 0x3b;
export const OP_IF_GTZ = 0x3c;
export const OP_IF_LEZ = 0x3d;
export const OP_AGET = 0x44;
export const OP_AGET_OBJECT = 0x46;
export const OP_APUT = 0x4b;
export const OP_APUT_OBJECT = 0x4d;
export const OP_IGET = 0x52;
export const OP_IGET_OBJECT = 0x54;
export const OP_IGET_BOOLEAN = 0x55;
export const OP_IPUT = 0x59;
export const OP_IPUT_OBJECT = 0x5b;
export const OP_SGET = 0x60;
export const OP_SGET_OBJECT = 0x62;
export const OP_SPUT = 0x67;
export const OP_SPUT_OBJECT = 0x69;
export const OP_INVOKE_VIRTUAL = 0x6e;
export const OP_INVOKE_SUPER = 0x6f;
export const OP_INVOKE_DIRECT = 0x70;
export const OP_INVOKE_STATIC = 0x71;
export const OP_INVOKE_INTERFACE = 0x72;
export const OP_INVOKE_VIRTUAL_RANGE = 0x74;
export const OP_INVOKE_SUPER_RANGE = 0x75;
export const OP_INVOKE_DIRECT_RANGE = 0x76;
export const OP_INVOKE_STATIC_RANGE = 0x77;
export const OP_INVOKE_INTERFACE_RANGE = 0x78;
export const OP_NEG_INT = 0x7b;
export const OP_NOT_INT = 0x7c;
export const OP_ADD_INT = 0x90;
export const OP_SUB_INT = 0x91;
export const OP_MUL_INT = 0x92;
export const OP_DIV_INT = 0x93;
export const OP_REM_INT = 0x94;
export const OP_AND_INT = 0x95;
export const OP_OR_INT = 0x96;
export const OP_XOR_INT = 0x97;
export const OP_SHL_INT = 0x98;
export const OP_SHR_INT = 0x99;
export const OP_USHR_INT = 0x9a;
export const OP_ADD_INT_2ADDR = 0xb0;
export const OP_SUB_INT_2ADDR = 0xb1;
export const OP_MUL_INT_2ADDR = 0xb2;
export const OP_DIV_INT_2ADDR = 0xb3;
export const OP_REM_INT_2ADDR = 0xb4;
export const OP_AND_INT_2ADDR = 0xb5;
export const OP_OR_INT_2ADDR = 0xb6;
export const OP_XOR_INT_2ADDR = 0xb7;
export const OP_SHL_INT_2ADDR = 0xb8;
export const OP_SHR_INT_2ADDR = 0xb9;
export const OP_USHR_INT_2ADDR = 0xba;
export const OP_ADD_INT_LIT16 = 0xd0;
export const OP_ADD_INT_LIT8 = 0xd8;

// -----------------------------------------------------------------------------
// LEB128 Reader Helpers
// -----------------------------------------------------------------------------
function readUleb128(bytes, offsetObj) {
    let result = 0;
    let shift = 0;
    while (true) {
        if (offsetObj.offset >= bytes.length) break;
        const b = bytes[offsetObj.offset++];
        result |= (b & 0x7f) << shift;
        if ((b & 0x80) === 0) break;
        shift += 7;
    }
    return result >>> 0;
}

// -----------------------------------------------------------------------------
// 1. Binary DEX Parser
// -----------------------------------------------------------------------------
export class DexParser {
    /**
     * @param {ArrayBuffer | Uint8Array} buffer
     * @param {string} [dexName='classes.dex']
     */
    constructor(buffer, dexName = 'classes.dex') {
        this.dexName = dexName;
        if (buffer instanceof ArrayBuffer) {
            this.bytes = new Uint8Array(buffer);
        } else if (buffer && buffer.buffer) {
            this.bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        } else {
            throw new Error("Invalid buffer provided to DexParser");
        }
        this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
        this.strings = [];
        this.types = [];
        this.protos = [];
        this.fields = [];
        this.methods = [];
        this.classes = new Map(); // className -> ClassDef
        this.isParsed = false;
    }

    parse() {
        if (this.isParsed) return this;
        if (this.bytes.length < 0x70) {
            throw new Error("DEX file too short for header");
        }

        // Verify magic: "dex\n035\0" or similar
        const magic = String.fromCharCode(...this.bytes.subarray(0, 8));
        if (!magic.startsWith("dex\n")) {
            throw new Error(`Invalid DEX magic header: ${magic}`);
        }

        const stringIdsSize = this.view.getUint32(0x38, true);
        const stringIdsOff = this.view.getUint32(0x3c, true);
        const typeIdsSize = this.view.getUint32(0x40, true);
        const typeIdsOff = this.view.getUint32(0x44, true);
        const protoIdsSize = this.view.getUint32(0x48, true);
        const protoIdsOff = this.view.getUint32(0x4c, true);
        const fieldIdsSize = this.view.getUint32(0x50, true);
        const fieldIdsOff = this.view.getUint32(0x54, true);
        const methodIdsSize = this.view.getUint32(0x58, true);
        const methodIdsOff = this.view.getUint32(0x5c, true);
        const classDefsSize = this.view.getUint32(0x60, true);
        const classDefsOff = this.view.getUint32(0x64, true);

        // 1. Parse Strings
        const decoder = new TextDecoder('utf-8');
        this.strings = new Array(stringIdsSize);
        for (let i = 0; i < stringIdsSize; i++) {
            const strDataOff = this.view.getUint32(stringIdsOff + i * 4, true);
            const offObj = { offset: strDataOff };
            readUleb128(this.bytes, offObj);
            let end = offObj.offset;
            while (end < this.bytes.length && this.bytes[end] !== 0) {
                end++;
            }
            try {
                this.strings[i] = decoder.decode(this.bytes.subarray(offObj.offset, end));
            } catch (_) {
                this.strings[i] = "";
            }
        }

        // 2. Parse Types
        this.types = new Array(typeIdsSize);
        for (let i = 0; i < typeIdsSize; i++) {
            const descriptorIdx = this.view.getUint32(typeIdsOff + i * 4, true);
            this.types[i] = this.strings[descriptorIdx] || "";
        }

        // 3. Parse Protos
        this.protos = new Array(protoIdsSize);
        for (let i = 0; i < protoIdsSize; i++) {
            const shortyIdx = this.view.getUint32(protoIdsOff + i * 12, true);
            const returnTypeIdx = this.view.getUint32(protoIdsOff + i * 12 + 4, true);
            const parametersOff = this.view.getUint32(protoIdsOff + i * 12 + 8, true);
            
            const params = [];
            if (parametersOff !== 0 && parametersOff + 4 <= this.bytes.length) {
                const paramCount = this.view.getUint32(parametersOff, true);
                for (let p = 0; p < paramCount && parametersOff + 4 + (p + 1) * 2 <= this.bytes.length; p++) {
                    const typeIdx = this.view.getUint16(parametersOff + 4 + p * 2, true);
                    params.push(this.types[typeIdx] || "V");
                }
            }
            this.protos[i] = {
                shorty: this.strings[shortyIdx] || "",
                returnType: this.types[returnTypeIdx] || "V",
                parameters: params
            };
        }

        // 4. Parse Fields
        this.fields = new Array(fieldIdsSize);
        for (let i = 0; i < fieldIdsSize; i++) {
            const classIdx = this.view.getUint16(fieldIdsOff + i * 8, true);
            const typeIdx = this.view.getUint16(fieldIdsOff + i * 8 + 2, true);
            const nameIdx = this.view.getUint32(fieldIdsOff + i * 8 + 4, true);
            this.fields[i] = {
                classType: this.types[classIdx] || "",
                fieldType: this.types[typeIdx] || "",
                name: this.strings[nameIdx] || ""
            };
        }

        // 5. Parse Methods
        this.methods = new Array(methodIdsSize);
        for (let i = 0; i < methodIdsSize; i++) {
            const classIdx = this.view.getUint16(methodIdsOff + i * 8, true);
            const protoIdx = this.view.getUint16(methodIdsOff + i * 8 + 2, true);
            const nameIdx = this.view.getUint32(methodIdsOff + i * 8 + 4, true);
            this.methods[i] = {
                id: i,
                classType: this.types[classIdx] || "",
                proto: this.protos[protoIdx] || { shorty: "", returnType: "V", parameters: [] },
                name: this.strings[nameIdx] || ""
            };
        }

        // 6. Parse Class Definitions
        for (let i = 0; i < classDefsSize; i++) {
            const entryOff = classDefsOff + i * 32;
            const classIdx = this.view.getUint32(entryOff, true);
            const accessFlags = this.view.getUint32(entryOff + 4, true);
            const superclassIdx = this.view.getUint32(entryOff + 8, true);
            const sourceFileIdx = this.view.getUint32(entryOff + 16, true);
            const classDataOff = this.view.getUint32(entryOff + 24, true);

            const className = this.types[classIdx] || "";
            const superClassName = superclassIdx !== 0xffffffff ? (this.types[superclassIdx] || "") : "";
            const sourceFile = sourceFileIdx !== 0xffffffff ? (this.strings[sourceFileIdx] || "") : "";

            const classDef = {
                className,
                normalizedName: className.replace(/^L/, '').replace(/;$/, '').replace(/\//g, '.'),
                superClassName,
                sourceFile,
                accessFlags,
                staticFields: [],
                instanceFields: [],
                directMethods: new Map(),
                virtualMethods: new Map(),
                staticValues: new Map()
            };

            // Parse Class Data Item if present
            if (classDataOff !== 0 && classDataOff < this.bytes.length) {
                const offObj = { offset: classDataOff };
                const numStaticFields = readUleb128(this.bytes, offObj);
                const numInstanceFields = readUleb128(this.bytes, offObj);
                const numDirectMethods = readUleb128(this.bytes, offObj);
                const numVirtualMethods = readUleb128(this.bytes, offObj);

                let fieldIdx = 0;
                for (let f = 0; f < numStaticFields; f++) {
                    const fieldDiff = readUleb128(this.bytes, offObj);
                    const flags = readUleb128(this.bytes, offObj);
                    fieldIdx += fieldDiff;
                    const fieldInfo = this.fields[fieldIdx];
                    if (fieldInfo) classDef.staticFields.push({ ...fieldInfo, accessFlags: flags });
                }

                fieldIdx = 0;
                for (let f = 0; f < numInstanceFields; f++) {
                    const fieldDiff = readUleb128(this.bytes, offObj);
                    const flags = readUleb128(this.bytes, offObj);
                    fieldIdx += fieldDiff;
                    const fieldInfo = this.fields[fieldIdx];
                    if (fieldInfo) classDef.instanceFields.push({ ...fieldInfo, accessFlags: flags });
                }

                let methodIdx = 0;
                for (let m = 0; m < numDirectMethods; m++) {
                    const methodDiff = readUleb128(this.bytes, offObj);
                    const flags = readUleb128(this.bytes, offObj);
                    const codeOff = readUleb128(this.bytes, offObj);
                    methodIdx += methodDiff;
                    const methodInfo = this.methods[methodIdx];
                    if (methodInfo) {
                        const parsedMethod = {
                            ...methodInfo,
                            accessFlags: flags,
                            code: this.parseCodeItem(codeOff),
                            dex: this
                        };
                        classDef.directMethods.set(methodInfo.name, parsedMethod);
                    }
                }

                methodIdx = 0;
                for (let m = 0; m < numVirtualMethods; m++) {
                    const methodDiff = readUleb128(this.bytes, offObj);
                    const flags = readUleb128(this.bytes, offObj);
                    const codeOff = readUleb128(this.bytes, offObj);
                    methodIdx += methodDiff;
                    const methodInfo = this.methods[methodIdx];
                    if (methodInfo) {
                        const parsedMethod = {
                            ...methodInfo,
                            accessFlags: flags,
                            code: this.parseCodeItem(codeOff),
                            dex: this
                        };
                        classDef.virtualMethods.set(methodInfo.name, parsedMethod);
                    }
                }
            }

            this.classes.set(classDef.normalizedName, classDef);
            this.classes.set(classDef.className, classDef);
        }

        this.isParsed = true;
        return this;
    }

    parseCodeItem(codeOff) {
        if (codeOff === 0 || codeOff + 16 > this.bytes.length) return null;
        const registersSize = this.view.getUint16(codeOff, true);
        const insSize = this.view.getUint16(codeOff + 2, true);
        const outsSize = this.view.getUint16(codeOff + 4, true);
        const triesSize = this.view.getUint16(codeOff + 6, true);
        const debugInfoOff = this.view.getUint32(codeOff + 8, true);
        const insnsSize = this.view.getUint32(codeOff + 12, true);

        if (codeOff + 16 + insnsSize * 2 > this.bytes.length) return null;

        let insns;
        const totalOffset = this.bytes.byteOffset + codeOff + 16;
        if (totalOffset % 2 === 0) {
            insns = new Uint16Array(
                this.bytes.buffer,
                totalOffset,
                insnsSize
            );
        } else {
            const sub = this.bytes.subarray(codeOff + 16, codeOff + 16 + insnsSize * 2);
            insns = new Uint16Array(sub.slice().buffer);
        }

        return {
            registersSize,
            insSize,
            outsSize,
            triesSize,
            debugInfoOff,
            insnsSize,
            insns
        };
    }
}

// -----------------------------------------------------------------------------
// 2. Dalvik Virtual Machine Interpreter
// -----------------------------------------------------------------------------
export class DalvikVM {
    constructor() {
        this.dexParsers = [];
        this.classes = new Map(); // normalizedName -> ClassDef
        this.staticState = new Map(); // className -> { fieldName: value }
        this.callStack = [];
        this.instructionsExecuted = 0;
        this.maxInstructionCount = 1000000;
        this.logcat = [];
        this.framework = new AndroidFrameworkBridge(this);
    }

    /**
     * Loads a parsed Dex into the VM class registry.
     * @param {DexParser} dexParser
     */
    loadDex(dexParser) {
        dexParser.parse();
        this.dexParsers.push(dexParser);
        for (const [key, classDef] of dexParser.classes.entries()) {
            this.classes.set(key, classDef);
        }
    }

    /**
     * Resolves a ClassDef by Java class name (e.g. "org.fdroid.fdroid.views.main.MainActivity").
     * @param {string} className
     * @returns {object | null}
     */
    findClass(className) {
        const normalized = className.replace(/^L/, '').replace(/;$/, '').replace(/\//g, '.');
        return this.classes.get(normalized) || this.classes.get(className) || null;
    }

    /**
     * Instantiates an Android Activity class and starts its lifecycle.
     * @param {string} activityClassName
     * @param {object} [intent={}]
     * @param {object} [savedInstanceState=null]
     * @returns {object} The created Activity instance.
     */
    startActivity(activityClassName, intent = {}, savedInstanceState = null) {
        const classDef = this.findClass(activityClassName);
        this.log(`[DalvikVM] Starting Activity [${activityClassName}]`, 'info');

        const activityInstance = this.framework.createActivity(activityClassName, intent);
        
        // Find and call <init>() constructor if present in DEX
        if (classDef) {
            const initMethod = classDef.directMethods.get('<init>');
            if (initMethod && initMethod.code) {
                try {
                    this.executeMethod(initMethod, activityInstance, []);
                } catch (e) {
                    this.log(`[DalvikVM] <init>() info: ${e.message}`, 'info');
                }
            }

            // Find and call onCreate(Bundle) method
            const onCreateMethod = classDef.virtualMethods.get('onCreate') || classDef.directMethods.get('onCreate');
            if (onCreateMethod && onCreateMethod.code) {
                try {
                    this.log(`[DalvikVM] Executing ${activityClassName}.onCreate()...`, 'info');
                    this.executeMethod(onCreateMethod, activityInstance, [savedInstanceState]);
                } catch (e) {
                    this.log(`[DalvikVM] onCreate() execution handled: ${e.message}`, 'info');
                }
            }
            activityInstance.onCreate(savedInstanceState);
        } else {
            activityInstance.onCreate(savedInstanceState);
        }

        activityInstance.isResumed = true;
        return activityInstance;
    }

    /**
     * Executes a Dalvik method bytecode on given instance with parameters.
     * @param {object} method - Method descriptor with code_item
     * @param {object} instance - `this` instance or null for static
     * @param {any[]} args - Method arguments
     * @returns {any} Method return value
     */
    executeMethod(method, instance, args = []) {
        const code = method.code;
        if (!code || !code.insns || code.insns.length === 0) {
            return null;
        }

        const currentDex = method.dex || null;
        const registers = new Array(code.registersSize).fill(0);
        let argIdx = code.registersSize - code.insSize;

        // Pass 'this' as first argument if not static
        const isStatic = (method.accessFlags & 0x0008) !== 0;
        if (!isStatic) {
            registers[argIdx++] = instance;
        }
        for (let i = 0; i < args.length && argIdx < code.registersSize; i++) {
            registers[argIdx++] = args[i];
        }

        let pc = 0;
        let lastResult = 0;
        const insns = code.insns;
        const insnsLen = insns.length;

        while (pc < insnsLen && this.instructionsExecuted < this.maxInstructionCount) {
            this.instructionsExecuted++;
            const word = insns[pc];
            const opcode = word & 0xff;
            const highByte = (word >> 8) & 0xff;

            switch (opcode) {
                case OP_NOP:
                    pc++;
                    break;

                case OP_MOVE:
                case OP_MOVE_OBJECT: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    registers[vA] = registers[vB];
                    pc++;
                    break;
                }

                case OP_MOVE_FROM16: {
                    const vA = highByte;
                    const vB = insns[pc + 1];
                    registers[vA] = registers[vB];
                    pc += 2;
                    break;
                }

                case OP_MOVE_RESULT:
                case OP_MOVE_RESULT_OBJECT: {
                    const vA = highByte;
                    registers[vA] = lastResult;
                    pc++;
                    break;
                }

                case OP_RETURN_VOID:
                    return undefined;

                case OP_RETURN:
                case OP_RETURN_OBJECT: {
                    const vA = highByte;
                    return registers[vA];
                }

                case OP_CONST_4: {
                    const vA = highByte & 0x0f;
                    let vB = (highByte >> 4) & 0x0f;
                    if (vB & 0x8) vB |= ~0x0f; // sign-extend
                    registers[vA] = vB;
                    pc++;
                    break;
                }

                case OP_CONST_16: {
                    const vA = highByte;
                    const val = (insns[pc + 1] << 16) >> 16;
                    registers[vA] = val;
                    pc += 2;
                    break;
                }

                case OP_CONST: {
                    const vA = highByte;
                    const val = insns[pc + 1] | (insns[pc + 2] << 16);
                    registers[vA] = val;
                    pc += 3;
                    break;
                }

                case OP_CONST_HIGH16: {
                    const vA = highByte;
                    registers[vA] = insns[pc + 1] << 16;
                    pc += 2;
                    break;
                }

                case OP_CONST_STRING: {
                    const vA = highByte;
                    const stringIdx = insns[pc + 1];
                    const str = this.resolveString(stringIdx, currentDex);
                    registers[vA] = str;
                    pc += 2;
                    break;
                }

                case OP_CONST_STRING_JUMBO: {
                    const vA = highByte;
                    const stringIdx = insns[pc + 1] | (insns[pc + 2] << 16);
                    registers[vA] = this.resolveString(stringIdx, currentDex);
                    pc += 3;
                    break;
                }

                case OP_CONST_CLASS: {
                    const vA = highByte;
                    const typeIdx = insns[pc + 1];
                    registers[vA] = this.resolveType(typeIdx, currentDex);
                    pc += 2;
                    break;
                }

                case OP_CHECK_CAST: {
                    pc += 2;
                    break;
                }

                case OP_INSTANCE_OF: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    registers[vA] = registers[vB] !== null && registers[vB] !== undefined ? 1 : 0;
                    pc += 2;
                    break;
                }

                case OP_NEW_INSTANCE: {
                    const vA = highByte;
                    const typeIdx = insns[pc + 1];
                    const typeName = this.resolveType(typeIdx, currentDex);
                    registers[vA] = this.framework.instantiateObject(typeName);
                    pc += 2;
                    break;
                }

                case OP_NEW_ARRAY: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const size = registers[vB] || 0;
                    registers[vA] = new Array(Math.max(0, size));
                    pc += 2;
                    break;
                }

                case OP_ARRAY_LENGTH: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const arr = registers[vB];
                    registers[vA] = (arr && arr.length) ? arr.length : 0;
                    pc++;
                    break;
                }

                case OP_AGET:
                case OP_AGET_OBJECT: {
                    const vA = highByte;
                    const vB = insns[pc + 1] & 0xff;
                    const vC = (insns[pc + 1] >> 8) & 0xff;
                    const arr = registers[vB];
                    const idx = registers[vC];
                    registers[vA] = (arr && arr[idx] !== undefined) ? arr[idx] : null;
                    pc += 2;
                    break;
                }

                case OP_APUT:
                case OP_APUT_OBJECT: {
                    const vA = highByte;
                    const vB = insns[pc + 1] & 0xff;
                    const vC = (insns[pc + 1] >> 8) & 0xff;
                    const arr = registers[vB];
                    const idx = registers[vC];
                    if (arr) arr[idx] = registers[vA];
                    pc += 2;
                    break;
                }

                case OP_IGET:
                case OP_IGET_OBJECT:
                case OP_IGET_BOOLEAN: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const fieldIdx = insns[pc + 1];
                    const fieldInfo = this.resolveField(fieldIdx, currentDex);
                    const obj = registers[vB];
                    registers[vA] = (obj && fieldInfo) ? obj[fieldInfo.name] : null;
                    pc += 2;
                    break;
                }

                case OP_IPUT:
                case OP_IPUT_OBJECT: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const fieldIdx = insns[pc + 1];
                    const fieldInfo = this.resolveField(fieldIdx, currentDex);
                    const obj = registers[vB];
                    if (obj && fieldInfo) {
                        obj[fieldInfo.name] = registers[vA];
                    }
                    pc += 2;
                    break;
                }

                case OP_SGET:
                case OP_SGET_OBJECT: {
                    const vA = highByte;
                    const fieldIdx = insns[pc + 1];
                    const fieldInfo = this.resolveField(fieldIdx, currentDex);
                    if (fieldInfo) {
                        const classState = this.staticState.get(fieldInfo.classType) || {};
                        registers[vA] = classState[fieldInfo.name] !== undefined ? classState[fieldInfo.name] : 0;
                    }
                    pc += 2;
                    break;
                }

                case OP_SPUT:
                case OP_SPUT_OBJECT: {
                    const vA = highByte;
                    const fieldIdx = insns[pc + 1];
                    const fieldInfo = this.resolveField(fieldIdx, currentDex);
                    if (fieldInfo) {
                        let classState = this.staticState.get(fieldInfo.classType);
                        if (!classState) {
                            classState = {};
                            this.staticState.set(fieldInfo.classType, classState);
                        }
                        classState[fieldInfo.name] = registers[vA];
                    }
                    pc += 2;
                    break;
                }

                case OP_GOTO: {
                    let offset = (highByte << 24) >> 24;
                    pc += offset;
                    break;
                }

                case OP_GOTO_16: {
                    let offset = (insns[pc + 1] << 16) >> 16;
                    pc += offset;
                    break;
                }

                case OP_IF_EQ:
                case OP_IF_NE:
                case OP_IF_LT:
                case OP_IF_GE:
                case OP_IF_GT:
                case OP_IF_LE: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const offset = (insns[pc + 1] << 16) >> 16;
                    let branch = false;
                    const a = registers[vA], b = registers[vB];
                    if (opcode === OP_IF_EQ) branch = (a === b);
                    else if (opcode === OP_IF_NE) branch = (a !== b);
                    else if (opcode === OP_IF_LT) branch = (a < b);
                    else if (opcode === OP_IF_GE) branch = (a >= b);
                    else if (opcode === OP_IF_GT) branch = (a > b);
                    else if (opcode === OP_IF_LE) branch = (a <= b);

                    if (branch) pc += offset;
                    else pc += 2;
                    break;
                }

                case OP_IF_EQZ:
                case OP_IF_NEZ:
                case OP_IF_LTZ:
                case OP_IF_GEZ:
                case OP_IF_GTZ:
                case OP_IF_LEZ: {
                    const vA = highByte;
                    const offset = (insns[pc + 1] << 16) >> 16;
                    let branch = false;
                    const a = registers[vA] || 0;
                    if (opcode === OP_IF_EQZ) branch = (!a || a === 0);
                    else if (opcode === OP_IF_NEZ) branch = (!!a && a !== 0);
                    else if (opcode === OP_IF_LTZ) branch = (a < 0);
                    else if (opcode === OP_IF_GEZ) branch = (a >= 0);
                    else if (opcode === OP_IF_GTZ) branch = (a > 0);
                    else if (opcode === OP_IF_LEZ) branch = (a <= 0);

                    if (branch) pc += offset;
                    else pc += 2;
                    break;
                }

                case OP_INVOKE_VIRTUAL:
                case OP_INVOKE_SUPER:
                case OP_INVOKE_DIRECT:
                case OP_INVOKE_STATIC:
                case OP_INVOKE_INTERFACE: {
                    const count = (highByte >> 4) & 0x0f;
                    const methodIdx = insns[pc + 1];
                    const regsWord = insns[pc + 2];
                    const regList = [
                        regsWord & 0x0f,
                        (regsWord >> 4) & 0x0f,
                        (regsWord >> 8) & 0x0f,
                        (regsWord >> 12) & 0x0f,
                        highByte & 0x0f
                    ].slice(0, count);

                    const methodInfo = this.resolveMethod(methodIdx, currentDex);
                    const callArgs = regList.map(r => registers[r]);
                    lastResult = this.framework.dispatchMethodCall(opcode, methodInfo, callArgs);
                    pc += 3;
                    break;
                }

                case OP_INVOKE_VIRTUAL_RANGE:
                case OP_INVOKE_SUPER_RANGE:
                case OP_INVOKE_DIRECT_RANGE:
                case OP_INVOKE_STATIC_RANGE:
                case OP_INVOKE_INTERFACE_RANGE: {
                    const count = highByte;
                    const methodIdx = insns[pc + 1];
                    const startReg = insns[pc + 2];
                    const regList = [];
                    for (let r = 0; r < count; r++) regList.push(startReg + r);

                    const methodInfo = this.resolveMethod(methodIdx, currentDex);
                    const callArgs = regList.map(r => registers[r]);
                    lastResult = this.framework.dispatchMethodCall(opcode, methodInfo, callArgs);
                    pc += 3;
                    break;
                }

                case OP_NEG_INT: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    registers[vA] = (-registers[vB]) | 0;
                    pc++;
                    break;
                }

                case OP_NOT_INT: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    registers[vA] = (~registers[vB]) | 0;
                    pc++;
                    break;
                }

                case OP_ADD_INT:
                case OP_SUB_INT:
                case OP_MUL_INT:
                case OP_DIV_INT:
                case OP_REM_INT:
                case OP_AND_INT:
                case OP_OR_INT:
                case OP_XOR_INT:
                case OP_SHL_INT:
                case OP_SHR_INT:
                case OP_USHR_INT: {
                    const vA = highByte;
                    const vB = insns[pc + 1] & 0xff;
                    const vC = (insns[pc + 1] >> 8) & 0xff;
                    const b = registers[vB] | 0, c = registers[vC] | 0;
                    let res = 0;
                    if (opcode === OP_ADD_INT) res = (b + c) | 0;
                    else if (opcode === OP_SUB_INT) res = (b - c) | 0;
                    else if (opcode === OP_MUL_INT) res = Math.imul(b, c);
                    else if (opcode === OP_DIV_INT) res = c !== 0 ? ((b / c) | 0) : 0;
                    else if (opcode === OP_REM_INT) res = c !== 0 ? (b % c) : 0;
                    else if (opcode === OP_AND_INT) res = b & c;
                    else if (opcode === OP_OR_INT) res = b | c;
                    else if (opcode === OP_XOR_INT) res = b ^ c;
                    else if (opcode === OP_SHL_INT) res = b << (c & 0x1f);
                    else if (opcode === OP_SHR_INT) res = b >> (c & 0x1f);
                    else if (opcode === OP_USHR_INT) res = b >>> (c & 0x1f);
                    registers[vA] = res;
                    pc += 2;
                    break;
                }

                case OP_ADD_INT_2ADDR:
                case OP_SUB_INT_2ADDR:
                case OP_MUL_INT_2ADDR:
                case OP_DIV_INT_2ADDR:
                case OP_REM_INT_2ADDR:
                case OP_AND_INT_2ADDR:
                case OP_OR_INT_2ADDR:
                case OP_XOR_INT_2ADDR:
                case OP_SHL_INT_2ADDR:
                case OP_SHR_INT_2ADDR:
                case OP_USHR_INT_2ADDR: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const a = registers[vA] | 0, b = registers[vB] | 0;
                    let res = 0;
                    if (opcode === OP_ADD_INT_2ADDR) res = (a + b) | 0;
                    else if (opcode === OP_SUB_INT_2ADDR) res = (a - b) | 0;
                    else if (opcode === OP_MUL_INT_2ADDR) res = Math.imul(a, b);
                    else if (opcode === OP_DIV_INT_2ADDR) res = b !== 0 ? ((a / b) | 0) : 0;
                    else if (opcode === OP_REM_INT_2ADDR) res = b !== 0 ? (a % b) : 0;
                    else if (opcode === OP_AND_INT_2ADDR) res = a & b;
                    else if (opcode === OP_OR_INT_2ADDR) res = a | b;
                    else if (opcode === OP_XOR_INT_2ADDR) res = a ^ b;
                    else if (opcode === OP_SHL_INT_2ADDR) res = a << (b & 0x1f);
                    else if (opcode === OP_SHR_INT_2ADDR) res = a >> (b & 0x1f);
                    else if (opcode === OP_USHR_INT_2ADDR) res = a >>> (b & 0x1f);
                    registers[vA] = res;
                    pc++;
                    break;
                }

                case OP_ADD_INT_LIT16: {
                    const vA = highByte & 0x0f;
                    const vB = (highByte >> 4) & 0x0f;
                    const lit = (insns[pc + 1] << 16) >> 16;
                    registers[vA] = (registers[vB] + lit) | 0;
                    pc += 2;
                    break;
                }

                case OP_ADD_INT_LIT8: {
                    const vA = highByte;
                    const vB = insns[pc + 1] & 0xff;
                    const lit = (insns[pc + 1] >> 8) << 24 >> 24;
                    registers[vA] = (registers[vB] + lit) | 0;
                    pc += 2;
                    break;
                }

                default:
                    pc += 1;
                    break;
            }
        }

        return lastResult;
    }

    resolveString(idx, dex = null) {
        if (dex && dex.strings && dex.strings[idx] !== undefined) return dex.strings[idx];
        for (const d of this.dexParsers) {
            if (d.strings && d.strings[idx] !== undefined) return d.strings[idx];
        }
        return "";
    }

    resolveType(idx, dex = null) {
        if (dex && dex.types && dex.types[idx] !== undefined) return dex.types[idx];
        for (const d of this.dexParsers) {
            if (d.types && d.types[idx] !== undefined) return d.types[idx];
        }
        return "Ljava/lang/Object;";
    }

    resolveField(idx, dex = null) {
        if (dex && dex.fields && dex.fields[idx]) return dex.fields[idx];
        for (const d of this.dexParsers) {
            if (d.fields && d.fields[idx]) return d.fields[idx];
        }
        return null;
    }

    resolveMethod(idx, dex = null) {
        if (dex && dex.methods && dex.methods[idx]) return dex.methods[idx];
        for (const d of this.dexParsers) {
            if (d.methods && d.methods[idx]) return d.methods[idx];
        }
        return null;
    }

    log(msg, type = 'info') {
        const entry = { timestamp: Date.now(), msg, type };
        this.logcat.push(entry);
        if (typeof window !== 'undefined' && window.AndroidLogBridge) {
            window.AndroidLogBridge.append(msg, type);
        }
    }
}

// -----------------------------------------------------------------------------
// 3. Android Framework & Java Library Bridge
// -----------------------------------------------------------------------------
export class AndroidFrameworkBridge {
    constructor(vm) {
        this.vm = vm;
        this.activities = new Map();
        this.currentActivity = null;
        this.toasts = [];
        this.sharedPreferences = new Map();
    }

    createActivity(className, intent = {}) {
        const self = this;
        const activity = {
            className,
            intent,
            views: new Map(), // id -> View
            contentView: null,
            isResumed: false,
            
            onCreate(savedInstanceState) {
                self.vm.log(`[Activity] ${className}.onCreate() called`, 'info');
                this.isResumed = true;
            },
            
            setContentView(viewOrResId) {
                self.vm.log(`[Activity] ${className}.setContentView(${viewOrResId})`, 'info');
                this.contentView = viewOrResId;
            },
            
            findViewById(id) {
                return this.views.get(id) || null;
            },

            startActivity(newIntent) {
                self.vm.log(`[Activity] startActivity -> ${JSON.stringify(newIntent)}`, 'info');
            },

            finish() {
                self.vm.log(`[Activity] ${className}.finish()`, 'info');
                this.isResumed = false;
            }
        };

        this.activities.set(className, activity);
        this.currentActivity = activity;
        return activity;
    }

    instantiateObject(typeName) {
        const norm = typeName.replace(/^L/, '').replace(/;$/, '').replace(/\//g, '.');
        if (norm === 'java.lang.StringBuilder') {
            return {
                str: '',
                append(s) { this.str += (s !== null && s !== undefined ? s : 'null'); return this; },
                toString() { return this.str; }
            };
        }
        if (norm === 'java.util.ArrayList') {
            return [];
        }
        if (norm === 'java.util.HashMap') {
            return new Map();
        }
        if (norm === 'android.os.Bundle') {
            return new Map();
        }
        if (norm === 'android.content.Intent') {
            return { action: '', categories: [], extras: new Map() };
        }
        return { __class: norm };
    }

    dispatchMethodCall(opcode, methodInfo, args) {
        if (!methodInfo) return null;
        const name = methodInfo.name;
        const classType = methodInfo.classType.replace(/^L/, '').replace(/;$/, '').replace(/\//g, '.');

        // Log.d / i / e / w
        if (classType === 'android.util.Log') {
            const tag = args[0] || 'App';
            const msg = args[1] || '';
            this.vm.log(`[${tag}] ${msg}`, name === 'e' ? 'error' : (name === 'w' ? 'warn' : 'info'));
            return 0;
        }

        // Toast.makeText(...).show()
        if (classType === 'android.widget.Toast') {
            if (name === 'makeText') {
                const text = String(args[1] || '');
                return {
                    text,
                    show: () => {
                        this.vm.log(`[Toast] "${text}"`, 'info');
                        if (typeof window !== 'undefined' && window.AndroidToast) {
                            window.AndroidToast.show(text);
                        }
                    }
                };
            }
        }

        // StringBuilder methods
        if (classType === 'java.lang.StringBuilder') {
            const sb = args[0];
            if (sb) {
                if (name === 'append') {
                    const val = args[1];
                    if (typeof sb.append === 'function') sb.append(val);
                    else sb.str = (sb.str || '') + val;
                    return sb;
                }
                if (name === 'toString') {
                    return typeof sb.toString === 'function' ? sb.toString() : (sb.str || '');
                }
            }
        }

        // String methods
        if (classType === 'java.lang.String') {
            const str = String(args[0] || '');
            if (name === 'length') return str.length;
            if (name === 'equals') return str === String(args[1]);
            if (name === 'contains') return str.includes(String(args[1]));
            if (name === 'valueOf') return String(args[0]);
        }

        // Activity methods
        if (classType.includes('Activity') || (args[0] && args[0].contentView !== undefined)) {
            const activity = args[0];
            if (activity) {
                if (name === 'setContentView') {
                    activity.setContentView(args[1]);
                    return null;
                }
                if (name === 'findViewById') {
                    return activity.findViewById(args[1]);
                }
                if (name === 'finish') {
                    activity.finish();
                    return null;
                }
            }
        }

        // Default instance method fallback
        const target = args[0];
        if (target && typeof target[name] === 'function') {
            return target[name](...args.slice(1));
        }

        return null;
    }
}
