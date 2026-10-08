use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

fn little_hex(hex: &str) -> u128 {
    assert_eq!(hex.len(), 32);
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    u128::from_le_bytes(bytes)
}

fn seed(o: &mut Orchestrator, word: u32) {
    let cpu = &mut o.machine.cpu[0];
    cpu.pc = RAM_BASE;
    cpu.pstate = 0x4000_03c5;
    cpu.sysregs.cpacr_el1 = 3 << 20;
    o.machine.ram[..4].copy_from_slice(&word.to_le_bytes());
}

fn expected_pattern_vector(start: usize, active_bytes: usize) -> u128 {
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().take(active_bytes).enumerate() {
        *byte = (start + index) as u8;
    }
    u128::from_le_bytes(bytes)
}

fn run_vector(o: &mut Orchestrator, word: u32, v0: &str, v1: &str) -> u128 {
    seed(o, word);
    o.machine.cpu[0].qregs[0] = little_hex(v0);
    o.machine.cpu[0].qregs[1] = little_hex(v1);
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    o.machine.cpu[0].qregs[2]
}

#[test]
fn t112_neon_arithmetic_matches_qemu_bytes_for_every_arrangement() {
    let v0 = "ff0001107f80fe021020304050607080";
    let v1 = "01010220018002fe0203040506070809";
    let cases = [
        (0x0e21_8402, "00010330800000000000000000000000"),
        (0x4e21_8402, "00010330800000001223344556677889"),
        (0x0e61_8402, "00020330800000010000000000000000"),
        (0x4e61_8402, "00020330800000011223344556677889"),
        (0x0ea1_8402, "00020330800001010000000000000000"),
        (0x4ea1_8402, "00020330800001011223344556677889"),
        (0x4ee1_8402, "00020330800001011223344556677889"),
        (0x2e21_8402, "fefffff07e00fc040000000000000000"),
        (0x6e21_8402, "fefffff07e00fc040e1d2c3b4a596877"),
        (0x2e61_8402, "feffffef7e00fc040000000000000000"),
        (0x6e61_8402, "feffffef7e00fc040e1d2c3b4a596877"),
        (0x2ea1_8402, "fefffeef7e00fc040000000000000000"),
        (0x6ea1_8402, "fefffeef7e00fc040e1d2c3b4a596877"),
        (0x6ee1_8402, "fefffeef7d00fc040e1d2c3b4a596877"),
        (0x0e21_9c02, "ff0002007f00fcfc0000000000000000"),
        (0x4e21_9c02, "ff0002007f00fcfc2060c040e0a08080"),
        (0x0e61_9c02, "ffff02407f00fc090000000000000000"),
        (0x4e61_9c02, "ffff02407f00fc092070c0f0e07180f3"),
        (0x0ea1_9c02, "fffffff27f003c460000000000000000"),
        (0x4ea1_9c02, "fffffff27f003c46207000e1e071c4e7"),
    ];
    let mut o = Orchestrator::new();
    for (word, expected) in cases {
        assert_eq!(
            run_vector(&mut o, word, v0, v1),
            little_hex(expected),
            "{word:#010x}"
        );
    }
}

#[test]
fn t112_dup_and_ext_match_qemu_bytes() {
    let mut o = Orchestrator::new();

    seed(&mut o, 0x4ea0_1c08); // MOV V8.16B, V0.16B
    o.machine.cpu[0].qregs[0] = little_hex("00112233445566778899aabbccddeeff");
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(
        o.machine.cpu[0].qregs[8],
        little_hex("00112233445566778899aabbccddeeff")
    );

    for (word, rd, immediate) in [(0x4f00_e440, 0, 2u8), (0x4f00_e461, 1, 3u8)] {
        seed(&mut o, word);
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(
            o.machine.cpu[0].qregs[rd],
            u128::from_le_bytes([immediate; 16]),
            "{word:#010x}"
        );
    }

    seed(&mut o, 0x4e01_0c22);
    o.machine.cpu[0].regs[1] = 0xa5b6_c7d8;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(
        o.machine.cpu[0].qregs[2],
        little_hex("d8d8d8d8d8d8d8d8d8d8d8d8d8d8d8d8")
    );

    for (word, input, expected) in [
        (
            0x4e0e_0402,
            "ff0001107f80fe021020304050607080",
            "fe02fe02fe02fe02fe02fe02fe02fe02",
        ),
        (
            0x4e18_0402,
            "ff0001107f80fe021020304050607080",
            "10203040506070801020304050607080",
        ),
    ] {
        seed(&mut o, word);
        o.machine.cpu[0].qregs[0] = little_hex(input);
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(
            o.machine.cpu[0].qregs[2],
            little_hex(expected),
            "{word:#010x}"
        );
    }

    seed(&mut o, 0x4e08_0ca2);
    o.machine.cpu[0].regs[5] = 0xa5b6_c7d8;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(
        o.machine.cpu[0].qregs[2],
        little_hex("d8c7b6a500000000d8c7b6a500000000")
    );

    for (word, expected) in [
        (0x6e01_3802, "02102030405060708001010220018002"),
        (0x6e01_7802, "8001010220018002fe02030405060708"),
    ] {
        seed(&mut o, word);
        o.machine.cpu[0].qregs[0] = little_hex("ff0001107f80fe021020304050607080");
        o.machine.cpu[0].qregs[1] = little_hex("01010220018002fe0203040506070809");
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(
            o.machine.cpu[0].qregs[2],
            little_hex(expected),
            "{word:#010x}"
        );
    }
}

#[test]
fn q_pair_transfers_support_writeback_sp_and_alignment_exceptions() {
    let mut o = Orchestrator::new();
    let base = RAM_BASE + 0x0100_0000;
    let offset = (base - RAM_BASE) as usize;
    let q0 = little_hex("000102030405060708090a0b0c0d0e0f");
    let q1 = little_hex("101112131415161718191a1b1c1d1e1f");
    o.machine.cpu[0].qregs[0] = q0;
    o.machine.cpu[0].qregs[1] = q1;
    o.machine.cpu[0].regs[9] = base;
    seed(&mut o, 0xad00_0520); // STP Q0, Q1, [X9]
    o.machine.cpu[0].qregs[0] = q0;
    o.machine.cpu[0].qregs[1] = q1;
    o.machine.cpu[0].regs[9] = base;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(
        &o.machine.ram[offset..offset + 32],
        &(q0.to_le_bytes()
            .into_iter()
            .chain(q1.to_le_bytes())
            .collect::<Vec<_>>())
    );

    seed(&mut o, 0xac81_0520); // STP Q0, Q1, [X9], #32
    o.machine.cpu[0].qregs[0] = q0;
    o.machine.cpu[0].qregs[1] = q1;
    o.machine.cpu[0].regs[9] = base + 0x100;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(o.machine.cpu[0].regs[9], base + 0x120);

    seed(&mut o, 0xad40_07e0); // LDP Q0, Q1, [SP]
    o.machine.cpu[0].sp = base;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(
        [o.machine.cpu[0].qregs[0], o.machine.cpu[0].qregs[1]],
        [q0, q1]
    );
    assert_eq!(o.machine.cpu[0].sp, base);

    seed(&mut o, 0xacc1_07e0); // LDP Q0, Q1, [SP], #32
    o.machine.cpu[0].sp = base;
    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(o.machine.cpu[0].sp, base + 32);

    let unaligned = base + 1;
    for (word, esr) in [(0xad40_2548, 0x9600_0021), (0xad00_2548, 0x9600_0061)] {
        seed(&mut o, word);
        o.machine.cpu[0].sysregs.vbar_el1 = 0x4020_0800;
        o.machine.cpu[0].regs[10] = unaligned;
        o.machine.cpu[0].sysregs.sctlr_el1 &= !2;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine.cpu[0].sysregs.esr_el1, esr);
        assert_eq!(o.machine.cpu[0].sysregs.far_el1, unaligned);
        assert_eq!(o.machine.cpu[0].sysregs.elr_el1, RAM_BASE);
        assert_eq!(o.machine.cpu[0].pc, 0x4020_0a00);
    }
}

#[test]
fn unallocated_mul_2d_matches_qemu_undefined_exception_entry() {
    let mut o = Orchestrator::new();
    let word = 0x4ee1_9c02; // MUL V2.2D, V0.2D, V1.2D is unallocated.
    assert_eq!(
        u1_decode::decode(word),
        pathn_contracts::cpu::DecodeResult::Illegal { word }
    );
    seed(&mut o, word);
    o.machine.cpu[0].sysregs.vbar_el1 = 0x4020_0800;

    assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
    assert_eq!(o.machine.cpu[0].sysregs.esr_el1, 0x0200_0000);
    assert_eq!(o.machine.cpu[0].sysregs.far_el1, 0);
    assert_eq!(o.machine.cpu[0].sysregs.elr_el1, RAM_BASE);
    assert_eq!(o.machine.cpu[0].pc, 0x4020_0a00);
}

#[test]
fn cpacr_fpen_traps_on_zero_and_two_and_allows_one_and_three() {
    let mut o = Orchestrator::new();
    for fpen in 0..4 {
        seed(&mut o, 0x4e01_0c20); // DUP V0.16B, W1
        o.machine.cpu[0].sysregs.vbar_el1 = 0x4020_0800;
        o.machine.cpu[0].regs[1] = 0x5a;
        o.machine.cpu[0].sysregs.cpacr_el1 = fpen << 20;
        if fpen & 1 == 0 {
            assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
            assert_eq!(o.machine.cpu[0].sysregs.esr_el1, 0x1fe0_0000);
            assert_eq!(o.machine.cpu[0].sysregs.elr_el1, RAM_BASE);
            assert_eq!(o.machine.cpu[0].pc, 0x4020_0a00);
        } else {
            assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
            assert_eq!(
                o.machine.cpu[0].qregs[0],
                little_hex("5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a")
            );
            assert_eq!(o.machine.cpu[0].pc, RAM_BASE + 4);
        }
    }
}

#[test]
fn fcmp_and_fcmpe_match_qemu_nzcv_and_fpsr_bytes() {
    let mut o = Orchestrator::new();
    let two = 2.0f64.to_bits() as u128;
    let quiet_nan = 0x7ff8_0000_0000_0000u64 as u128;
    let cases = [
        (0x1e61_2000, 1.0f64.to_bits() as u128, two), // FCMP D0, D1: less
        (0x1e60_2020, 1.0f64.to_bits() as u128, two), // FCMP D1, D0: greater
        (0x1e60_2010, two, two),                      // FCMPE D0, D0: equal
        (0x1e61_2000, quiet_nan, two),                // FCMP quiet NaN: unordered
        (0x1e61_2010, quiet_nan, two),                // FCMPE quiet NaN: invalid
    ];
    let mut actual = Vec::with_capacity(48);
    for (word, lhs, rhs) in cases {
        seed(&mut o, word);
        o.machine.cpu[0].qregs[0] = lhs;
        o.machine.cpu[0].qregs[1] = rhs;
        assert!(
            matches!(o.step_vcpu(), StepOutcome::Continue),
            "{word:#010x}"
        );
        actual.extend_from_slice(&(o.machine.cpu[0].pstate & (0xf << 28)).to_le_bytes());
    }
    actual.extend_from_slice(&o.machine.cpu[0].sysregs.fpsr.to_le_bytes());

    // Independent QEMU 11.1.2 capture from a bare-metal FCMP/FCMPE sequence:
    // 1<2, 2>1, 2==2, qNaN FCMP, qNaN FCMPE, then FPSR.
    let expected = [
        0x8000_0000u64,
        0x2000_0000,
        0x6000_0000,
        0x3000_0000,
        0x3000_0000,
        1,
    ]
    .into_iter()
    .flat_map(u64::to_le_bytes)
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn ld1_multi_structure_arrangements_and_post_index_match_qemu() {
    let mut o = Orchestrator::new();
    let base = RAM_BASE + 0x0100_0000;
    let data_offset = (base - RAM_BASE) as usize;
    for (index, byte) in o.machine.ram[data_offset..data_offset + 256]
        .iter_mut()
        .enumerate()
    {
        *byte = index as u8;
    }

    // The independent QEMU harness loads the same 0..255 byte pattern and
    // compares 19 resulting Q-registers plus all eight base values (480 B).
    let cases = [
        (0x0c40_7020, 1, 0, 1, 8, 0, None),
        (0x4cdf_a061, 3, 1, 2, 16, 16, Some((31, 32))),
        (0x0cdf_64c3, 6, 3, 3, 8, 48, Some((31, 24))),
        (0x4cca_27e6, 31, 6, 4, 16, 72, Some((10, 64))),
        (0x0c40_a98a, 12, 10, 2, 8, 136, None),
        (0x4cdf_2a2d, 17, 13, 4, 16, 144, Some((31, 64))),
        (0x0cdf_7eb2, 21, 18, 1, 8, 208, Some((31, 8))),
        (0x4cd7_aed4, 22, 20, 2, 16, 216, Some((23, 32))),
    ];

    for (word, rn, rt, count, vector_bytes, input_offset, post_index) in cases {
        seed(&mut o, word);
        let start = base + input_offset as u64;
        if rn == 31 {
            o.machine.cpu[0].sp = start;
        } else {
            o.machine.cpu[0].regs[rn] = start;
        }
        if let Some((rm, amount)) = post_index {
            if rm != 31 {
                o.machine.cpu[0].regs[rm] = amount;
            }
        }

        assert!(
            matches!(o.step_vcpu(), StepOutcome::Continue),
            "{word:#010x}"
        );
        for index in 0..count {
            assert_eq!(
                o.machine.cpu[0].qregs[rt + index],
                expected_pattern_vector(input_offset + index * vector_bytes, vector_bytes),
                "{word:#010x} V{}",
                rt + index
            );
        }
        let expected_base = post_index
            .map(|(_, amount)| start + amount)
            .unwrap_or(start);
        let actual_base = if rn == 31 {
            o.machine.cpu[0].sp
        } else {
            o.machine.cpu[0].regs[rn]
        };
        assert_eq!(actual_base, expected_base, "{word:#010x} base writeback");
    }
}

#[test]
fn st1_multi_structure_arrangements_and_post_index_match_qemu() {
    let mut o = Orchestrator::new();
    let output = RAM_BASE + 0x0100_0200;
    let output_offset = (output - RAM_BASE) as usize;
    o.machine.ram[output_offset..output_offset + 280].fill(0xa5);

    // QEMU writes these same 280 data bytes followed by nine base snapshots.
    let cases = [
        (0x0c00_7260, 19, 0, 1, 8, 0, None),
        (0x4c9f_a261, 19, 1, 2, 16, 16, Some((31, 32))),
        (0x0c9f_6663, 19, 3, 3, 8, 48, Some((31, 24))),
        (0x4c8a_27e6, 31, 6, 4, 16, 72, Some((10, 64))),
        (0x0c00_aa6a, 19, 10, 2, 8, 136, None),
        (0x4c9f_2a6d, 19, 13, 4, 16, 144, Some((31, 64))),
        (0x0c9f_7e72, 19, 18, 1, 8, 208, Some((31, 8))),
        (0x4c97_ae74, 19, 20, 2, 16, 216, Some((23, 32))),
        (0x4c00_aa74, 19, 20, 2, 16, 216, None), // Exact kernel ST1 encoding.
    ];

    let mut cursor = 0usize;
    let mut expected = Vec::with_capacity(280);
    for (word, rn, rt, count, vector_bytes, input_offset, post_index) in cases {
        seed(&mut o, word);
        for index in 0..count {
            let value = expected_pattern_vector(input_offset + index * vector_bytes, vector_bytes);
            o.machine.cpu[0].qregs[(rt + index) & 31] = value;
            expected.extend_from_slice(&value.to_le_bytes()[..vector_bytes]);
        }

        let start = output + cursor as u64;
        if rn == 31 {
            o.machine.cpu[0].sp = start;
        } else {
            o.machine.cpu[0].regs[rn] = start;
        }
        if let Some((rm, amount)) = post_index {
            if rm != 31 {
                o.machine.cpu[0].regs[rm] = amount;
            }
        }

        assert!(
            matches!(o.step_vcpu(), StepOutcome::Continue),
            "{word:#010x}"
        );
        let expected_base = post_index
            .map(|(_, amount)| start + amount)
            .unwrap_or(start);
        let actual_base = if rn == 31 {
            o.machine.cpu[0].sp
        } else {
            o.machine.cpu[0].regs[rn]
        };
        assert_eq!(actual_base, expected_base, "{word:#010x} base writeback");
        cursor += count * vector_bytes;
    }

    assert_eq!(cursor, 280);
    assert_eq!(
        &o.machine.ram[output_offset..output_offset + cursor],
        expected
    );
}

#[test]
fn rev16_rev32_and_kernel_rev64_match_qemu_bytes() {
    let mut o = Orchestrator::new();
    let source = little_hex("000102030405060708090a0b0c0d0e0f");
    let cases = [
        (0x0e20_1b19, "01000302050407060000000000000000"), // REV16 .8B
        (0x4e20_1b19, "010003020504070609080b0a0d0c0f0e"), // REV16 .16B
        (0x2e20_0b19, "03020100070605040000000000000000"), // REV32 .8B
        (0x6e20_0b19, "03020100070605040b0a09080f0e0d0c"), // REV32 .16B
        (0x2e60_0b19, "02030001060704050000000000000000"), // REV32 .4H
        (0x6e60_0b19, "02030001060704050a0b08090e0f0c0d"), // REV32 .8H
        (0x4e60_0b19, "06070405020300010e0f0c0d0a0b0809"), // REV64 .8H
    ];

    for (word, expected) in cases {
        seed(&mut o, word);
        o.machine.cpu[0].qregs[24] = source;
        assert!(
            matches!(o.step_vcpu(), StepOutcome::Continue),
            "{word:#010x}"
        );
        assert_eq!(
            o.machine.cpu[0].qregs[25],
            little_hex(expected),
            "{word:#010x}"
        );
    }
}

#[test]
fn sha256_crypto_instructions_match_qemu_bytes() {
    let mut o = Orchestrator::new();
    let vectors = [
        "0822cdfffeda5cbce9dacc3d1e171679",
        "d208e65695e2f20ef4233775db87c3fa",
        "2ecc1eaa5c2ed72736628fff04d87d21",
        "30f4adc81b89a27ec50798b8db34e4fd",
        "c420951ef9f9fcddab7efbbdc9b9652b",
        "275c0b2b313dcad1684a14fd7d9cc019",
        "78ecc4819f160802cb2f10cec751ac6c",
        "5e3c087c04e01fd629ba91b982b6b12b",
        "3343f25910c72acb3419b6405b11ba69",
        "5e3587f783db713e9f9a0f82e8e152eb",
        "ce500d21560cf64eea80281d4cd7577a",
    ]
    .map(little_hex);

    for (register, value) in vectors.into_iter().enumerate() {
        o.machine.cpu[0].qregs[register] = value;
    }

    let cases = [
        (0x5e28_2820, 0, "05b019dc9928d59f714be4f3d0af5ba5"), // SHA256SU0 V0.4S, V1.4S
        (0x5e04_4062, 2, "2d41f399db2891ba64b21cbe115b4d09"), // SHA256H Q2, Q3, V4.4S
        (0x5e07_50c5, 5, "ceca3fc5b1553771a5807d0e99a87a75"), // SHA256H2 Q5, Q6, V7.4S
        (0x5e0a_6128, 8, "c725d3e843898b5ed6386a229fc86540"), // SHA256SU1 V8.4S, V9.4S, V10.4S
    ];

    for (word, destination, expected) in cases {
        seed(&mut o, word);
        assert!(
            matches!(o.step_vcpu(), StepOutcome::Continue),
            "{word:#010x}"
        );
        assert_eq!(
            o.machine.cpu[0].qregs[destination],
            little_hex(expected),
            "{word:#010x}"
        );
    }
}
