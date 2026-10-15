# T499 matched-oracle acceptance gates

- [x] G499-1: Pinned guest inputs and runtime manifest validate
  CHECK: uv run --no-project python scripts/verify_manifest.py
  EXPECT: G499_1_EVIDENCE
  CWD: .
  OWNS: MANIFEST.json, scripts/verify_manifest.py
  EVIDENCE: exit_code=0 timestamp=2026-10-15T10:43:23.622049+00:00 match='G499_1_EVIDENCE' epoch=1 sig=22850501b7d3 record={"attempt_id": 1, "command_digest": "1f78a540ecff72e147f373880ea4d1d0fcc947eb4b3907a0ffd98c033a6fbe59", "environment_digest": "fdd1074df00a0e1a0479fa73ef1ae20d82113683b7b1b443db4778ba47a0a706", "gate_id": "G499-1", "gate_signature": "22850501b7d371a033f85b93a6ac5f1b49eca7d9be9c088a5e7716945d3df954", "match_preview": "G499_1_EVIDENCE", "run_epoch": 1, "run_id": "local_run", "timestamp": "2026-10-15T10:43:23.622049+00:00"}

- [x] G499-2: Guest kernel and initramfs markers are present in the direct UART capture
  CHECK: uv run --no-project python -c 'from pathlib import Path; s=Path("qemu-manifest-boot.log").read_text(errors="replace"); required=("Run /init as init process", "INITRAMFS ROOTFS MOUNTED AT / OK", "STATUS: PASS - USERSPACE INITIALIZATION COMPLETE"); assert all(x in s for x in required), "missing guest UART marker"; print("G499_2_EVIDENCE")'
  EXPECT: G499_2_EVIDENCE
  CWD: .
  OWNS: qemu-manifest-boot.log
  EVIDENCE: exit_code=0 timestamp=2026-10-15T10:43:23.654692+00:00 match='G499_2_EVIDENCE' epoch=1 sig=e48bfaf6a3a6 record={"attempt_id": 1, "command_digest": "0b7a8abdc286266f12d6e38575f6acbd82513462cd0ed35a04c9529c432680f3", "environment_digest": "fdd1074df00a0e1a0479fa73ef1ae20d82113683b7b1b443db4778ba47a0a706", "gate_id": "G499-2", "gate_signature": "e48bfaf6a3a609c34735524d17d619bba116dce865a1b5c954a43ff877b28f73", "match_preview": "G499_2_EVIDENCE", "run_epoch": 1, "run_id": "local_run", "timestamp": "2026-10-15T10:43:23.654692+00:00"}

- [x] G499-3: CSINV verdict has matched-state witnesses and guest quirks default off
  CHECK: uv run --no-project python scripts/verify_t499_result.py
  EXPECT: G499_3_EVIDENCE
  CWD: .
  OWNS: findings, evidence/t499, units/u12-orchestrator/src/lib.rs, units/u12-orchestrator/src/bin/run_kernel.rs
  EVIDENCE: exit_code=0 timestamp=2026-10-15T10:43:23.689716+00:00 match='G499_3_EVIDENCE' epoch=1 sig=5f7a598aaef2 record={"attempt_id": 1, "command_digest": "e4a5fa5d0315302076865bf6da8b9ee5c80c7676811b54890863df6dd8fd2839", "environment_digest": "fdd1074df00a0e1a0479fa73ef1ae20d82113683b7b1b443db4778ba47a0a706", "gate_id": "G499-3", "gate_signature": "5f7a598aaef22161547250bc5a9647d89bf6ff4ba612b5b8f2e9803615924cc9", "match_preview": "G499_3_EVIDENCE", "run_epoch": 1, "run_id": "local_run", "timestamp": "2026-10-15T10:43:23.689716+00:00"}
