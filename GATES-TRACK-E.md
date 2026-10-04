# Gates: Track E - Devices (GIC, Timer IRQ, Virtio-Console, Virtio-Blk)

OWNS: units/u5-gic-timer/**, units/u17-virtio-blk/**, units/u18-virtio-console/**, Cargo.toml, TIMELINE.md

Scope: Implement GICv2 interrupt controller, ARM generic timer IRQ, virtio-console, and virtio-blk device models with passing bare-metal tests.

- [ ] E1: GIC model implemented with bare-metal unit tests passing
  CHECK: cargo test -p u5-gic-timer -- gic
  EXPECT: test result: ok

- [ ] E2: Timer IRQ model and GIC PPI 27 integration tests passing
  CHECK: cargo test -p u5-gic-timer -- timer
  EXPECT: test result: ok

- [ ] E3: virtio-console device model with bare-metal unit tests passing
  CHECK: cargo test -p u18-virtio-console
  EXPECT: test result: ok

- [ ] E4: virtio-blk device model with bare-metal unit tests passing
  CHECK: cargo test -p u17-virtio-blk
  EXPECT: test result: ok

- [ ] E5: Full device suite bare-metal verification
  CHECK: cargo test -p u5-gic-timer -p u17-virtio-blk -p u18-virtio-console
  EXPECT: test result: ok

- [ ] E6: Result document /Users/Shared/track-e-result.md created with passing evidence
  CHECK: test -f /Users/Shared/track-e-result.md && echo "RESULT_EXISTS_OK"
  EXPECT: RESULT_EXISTS_OK
