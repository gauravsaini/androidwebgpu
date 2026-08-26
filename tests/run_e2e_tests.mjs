#!/usr/bin/env node
/**
 * androidwebgpu - Central E2E Test Suite Runner
 * 
 * Executes Tiers 1 through 4:
 * - Tier 1: Feature Coverage (35 tests)
 * - Tier 2: Boundary & Corner Conditions (35 tests)
 * - Tier 3: Cross-Feature Interactions (7 tests)
 * - Tier 4: Real-World Application Scenarios (5 tests)
 * 
 * Aggregates results, outputs a formatted summary table, and exits with code 0 on full pass.
 * Conforms to ASD-STE100 and /ponytail simplicity principles.
 */

import { runTier1Tests } from './e2e/tier1_feature_coverage.mjs';
import { runTier2Tests } from './e2e/tier2_boundary_corner.mjs';
import { runTier3Tests } from './e2e/tier3_cross_feature.mjs';
import { runTier4Tests } from './e2e/tier4_real_world.mjs';

async function main() {
    console.log("================================================================================");
    console.log("⚡ STARTING ANDROIDWEBGPU 4-TIER E2E TEST RUNNER");
    console.log("================================================================================");

    const startTime = performance.now();
    const tierResults = [];

    // Run Tier 1
    const t1 = await runTier1Tests();
    tierResults.push(t1);

    // Run Tier 2
    const t2 = await runTier2Tests();
    tierResults.push(t2);

    // Run Tier 3
    const t3 = await runTier3Tests();
    tierResults.push(t3);

    // Run Tier 4
    const t4 = await runTier4Tests();
    tierResults.push(t4);

    const duration = ((performance.now() - startTime) / 1000).toFixed(2);

    // Aggregate statistics
    let totalTests = 0;
    let totalPassed = 0;
    let totalFailed = 0;

    for (const res of tierResults) {
        totalTests += res.total;
        totalPassed += res.passed;
        totalFailed += res.failed;
    }

    console.log("\n================================================================================");
    console.log("📊 E2E TEST SUITE EXECUTION SUMMARY");
    console.log("================================================================================");
    console.log("┌───────────────────────────────────┬─────────┬─────────┬─────────┬──────────┐");
    console.log("│ Test Tier                         │ Total   │ Passed  │ Failed  │ Status   │");
    console.log("├───────────────────────────────────┼─────────┼─────────┼─────────┼──────────┤");

    for (const res of tierResults) {
        const tierName = res.tier.padEnd(33, ' ');
        const totalStr = String(res.total).padStart(7, ' ');
        const passedStr = String(res.passed).padStart(7, ' ');
        const failedStr = String(res.failed).padStart(7, ' ');
        const statusStr = res.failed === 0 ? " PASS     " : " FAIL     ";
        console.log(`│ ${tierName} │ ${totalStr} │ ${passedStr} │ ${failedStr} │ ${statusStr}│`);
    }

    console.log("├───────────────────────────────────┼─────────┼─────────┼─────────┼──────────┤");
    const totalLabel = "Total Suite Execution".padEnd(33, ' ');
    const totStr = String(totalTests).padStart(7, ' ');
    const passStr = String(totalPassed).padStart(7, ' ');
    const failStr = String(totalFailed).padStart(7, ' ');
    const overallStatus = totalFailed === 0 ? " PASS     " : " FAIL     ";
    console.log(`│ ${totalLabel} │ ${totStr} │ ${passStr} │ ${failStr} │ ${overallStatus}│`);
    console.log("└───────────────────────────────────┴─────────┴─────────┴─────────┴──────────┘");

    console.log(`\nExecution Time: ${duration}s`);
    console.log(`Final Result: ${totalPassed}/${totalTests} Tests Passed (100% Target Met)`);

    if (totalFailed > 0 || totalTests < 82) {
        console.error(`\n✖ E2E Test Suite FAILED with ${totalFailed} failures (Total: ${totalTests}, Required: >=82)`);
        process.exit(1);
    } else {
        console.log(`\n✔ E2E Test Suite PASSED cleanly with zero failures!`);
        process.exit(0);
    }
}

main().catch(err => {
    console.error("Fatal test runner error:", err);
    process.exit(1);
});
