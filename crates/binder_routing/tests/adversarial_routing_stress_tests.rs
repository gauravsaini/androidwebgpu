//! Adversarial stress test suite for `binder_routing`.
//!
//! Tests high-throughput concurrent routing evaluation, glob wildcard pathological patterns,
//! priority inversion resistance, opcode boundary filters, and policy mutation stress.

use binder_routing::{
    CodeFilter, DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, RoutingPolicy,
    RoutingRule, ServiceNameMatcher,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// Xorshift64 PRNG for fast reproducible test input generation.
struct ChaosRng {
    state: u64,
}

impl ChaosRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xCAFE_BABE_DEAD_BEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            min
        } else {
            min + (self.next_u64() % ((max - min + 1) as u64)) as usize
        }
    }
}

#[test]
fn test_concurrent_high_throughput_routing_evaluation() {
    let mut policy = RoutingPolicy::new_default_local();

    // Configure realistic Android surface offloading rules
    policy.add_rule(
        RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::HostOffload)
            .with_service_name("SurfaceFlinger")
            .with_priority(100),
    );
    policy.add_rule(
        RoutingRule::new("android.gui.IGraphicBufferProducer", RouteAction::HostOffload)
            .with_priority(90),
    );
    policy.add_rule(
        RoutingRule::new("android.gui.IDisplayEventConnection", RouteAction::Hybrid { host_codes: vec![1, 2] })
            .with_priority(80),
    );
    policy.add_rule(
        RoutingRule::new("android.view.*", RouteAction::LocalGuest)
            .with_priority(50),
    );
    policy.add_rule(
        RoutingRule::new("android.hardware.graphics.*", RouteAction::HostOffload)
            .with_priority(40),
    );

    let policy_arc = Arc::new(policy);
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);

    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Prefix("android.gui.".to_string()),
            RouteAction::HostOffload,
        )
        .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string()))
        .with_code_filter(CodeFilter::Range(1000, 1100))
        .with_priority(50),
    );

    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Wildcard("android.*.I*Producer".to_string()),
            RouteAction::HostOffload,
        )
        .with_code_filter(CodeFilter::Except(vec![99]))
        .with_priority(40),
    );

    let engine_arc = Arc::new(engine);

    let num_threads = 16;
    let lookups_per_thread = 15_000;
    let total_evaluations = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(num_threads);

    for t in 0..num_threads {
        let p = Arc::clone(&policy_arc);
        let e = Arc::clone(&engine_arc);
        let count = Arc::clone(&total_evaluations);

        let handle = thread::spawn(move || {
            let mut rng = ChaosRng::new(0x1000 + t as u64);
            let descriptors = [
                "android.gui.ISurfaceComposer",
                "android.gui.IGraphicBufferProducer",
                "android.gui.IDisplayEventConnection",
                "android.view.IWindowManager",
                "android.view.accessibility.IAccessibilityManager",
                "android.hardware.graphics.composer.IComposer",
                "android.os.IPowerManager",
                "android.os.IServiceManager",
                "custom.unknown.IFoo",
            ];
            let services = [
                Some("SurfaceFlinger"),
                Some("window"),
                Some("power"),
                None,
                Some("unknown_service"),
            ];

            for _ in 0..lookups_per_thread {
                let desc = descriptors[rng.next_range(0, descriptors.len() - 1)];
                let svc = services[rng.next_range(0, services.len() - 1)];
                let code = rng.next_u32() % 1200;

                // Evaluate policy
                let action_p = p.route_service(svc, desc, code);
                match desc {
                    "android.gui.ISurfaceComposer" if svc == Some("SurfaceFlinger") => {
                        assert_eq!(action_p, RouteAction::HostOffload);
                    }
                    "android.gui.IGraphicBufferProducer" => {
                        assert_eq!(action_p, RouteAction::HostOffload);
                    }
                    "android.gui.IDisplayEventConnection" => {
                        if code == 1 || code == 2 {
                            assert_eq!(action_p, RouteAction::HostOffload);
                        } else {
                            assert_eq!(action_p, RouteAction::LocalGuest);
                        }
                    }
                    "android.view.IWindowManager" | "android.view.accessibility.IAccessibilityManager" => {
                        assert_eq!(action_p, RouteAction::LocalGuest);
                    }
                    "android.hardware.graphics.composer.IComposer" => {
                        assert_eq!(action_p, RouteAction::HostOffload);
                    }
                    _ => {
                        // Unconfigured or non-matching routes to default (LocalGuest)
                        assert_eq!(action_p, RouteAction::LocalGuest);
                    }
                }

                // Evaluate engine
                let action_e = e.match_transaction(svc, Some(desc), code);
                if desc.starts_with("android.gui.") && svc == Some("SurfaceFlinger") && (1000..=1100).contains(&code) {
                    assert_eq!(action_e, RouteAction::HostOffload);
                }

                count.fetch_add(1, Ordering::Relaxed);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Concurrent evaluation thread must not panic");
    }

    assert_eq!(
        total_evaluations.load(Ordering::SeqCst),
        num_threads * lookups_per_thread
    );
}

#[test]
fn test_wildcard_glob_pathological_and_boundary_patterns() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);

    // Rule 1: Suffix glob
    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Wildcard("*Composer".to_string()),
            RouteAction::HostOffload,
        )
        .with_priority(10),
    );

    // Rule 2: Multi-star glob
    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Wildcard("android.*.gpu.*Service".to_string()),
            RouteAction::HostOffload,
        )
        .with_priority(20),
    );

    // Rule 3: Single star catch-all
    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Wildcard("*".to_string()),
            RouteAction::LocalGuest,
        )
        .with_priority(-100),
    );

    // Test Rule 1: Suffix
    assert_eq!(
        engine.match_transaction(None, Some("android.gui.ISurfaceComposer"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(None, Some("ISurfaceComposer"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(None, Some("Composer"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(None, Some("android.gui.ISurfaceComposer2"), 1),
        RouteAction::LocalGuest
    );

    // Test Rule 2: Multi-star glob
    assert_eq!(
        engine.match_transaction(None, Some("android.hardware.gpu.BufferService"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(None, Some("android.internal.sub.gpu.RenderingService"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(None, Some("android.gpu.BufferService"), 1),
        RouteAction::LocalGuest // Missing the middle segment between android and gpu
    );

    // Empty and degenerate strings
    assert_eq!(
        engine.match_transaction(None, Some(""), 1),
        RouteAction::LocalGuest
    );
    assert_eq!(
        engine.match_transaction(None, None, 1),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_priority_inversion_resistance_and_shadowing() {
    let mut policy = RoutingPolicy::new_default_local();

    // Add broad wildcard with low priority
    policy.add_rule(
        RoutingRule::new("android.gui.*", RouteAction::HostOffload)
            .with_priority(0),
    );

    // Add specific denial with higher priority
    policy.add_rule(
        RoutingRule::new("android.gui.IDebugInterface", RouteAction::LocalGuest)
            .with_priority(50),
    );

    // Add highest priority hybrid override
    policy.add_rule(
        RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::Hybrid { host_codes: vec![1006] })
            .with_priority(100),
    );

    // 1. Specific denial has higher priority than broad wildcard
    assert_eq!(
        policy.route("android.gui.IDebugInterface", 100),
        RouteAction::LocalGuest
    );

    // 2. Regular gui interface hits broad wildcard
    assert_eq!(
        policy.route("android.gui.IGraphicBufferProducer", 1),
        RouteAction::HostOffload
    );

    // 3. ISurfaceComposer hits highest priority hybrid rule
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.gui.ISurfaceComposer", 1020),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_code_filter_adversarial_boundaries() {
    // 1. Inverted range [min > max]
    let inverted = CodeFilter::Range(100, 50);
    assert!(!inverted.matches(75));
    assert!(!inverted.matches(100));
    assert!(!inverted.matches(50));

    // 2. Single value range
    let single = CodeFilter::Range(1006, 1006);
    assert!(single.matches(1006));
    assert!(!single.matches(1005));
    assert!(!single.matches(1007));

    // 3. Full range boundary [0..=u32::MAX]
    let full = CodeFilter::Range(0, u32::MAX);
    assert!(full.matches(0));
    assert!(full.matches(1006));
    assert!(full.matches(u32::MAX));

    // 4. Empty specific list
    let empty_spec = CodeFilter::Specific(vec![]);
    assert!(!empty_spec.matches(0));
    assert!(!empty_spec.matches(1));

    // 5. Empty except blacklist -> matches everything
    let empty_except = CodeFilter::Except(vec![]);
    assert!(empty_except.matches(0));
    assert!(empty_except.matches(u32::MAX));

    // 6. Saturated except blacklist
    let except_99 = CodeFilter::Except(vec![99, 100]);
    assert!(except_99.matches(0));
    assert!(except_99.matches(98));
    assert!(!except_99.matches(99));
    assert!(!except_99.matches(100));
    assert!(except_99.matches(101));
}

#[test]
fn test_policy_scaling_and_saturation() {
    let mut policy = RoutingPolicy::new_default_local();

    // Insert 1,000 distinct interface rules
    for i in 0..1000 {
        let desc = format!("android.test.interface.IService_{}", i);
        let action = if i % 2 == 0 {
            RouteAction::HostOffload
        } else {
            RouteAction::Hybrid { host_codes: vec![i as u32] }
        };
        policy.add_rule(RoutingRule::new(desc, action).with_priority(i));
    }

    assert_eq!(policy.len(), 1000);

    // Verify lookup correctness
    assert_eq!(
        policy.route("android.test.interface.IService_500", 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.test.interface.IService_501", 501),
        RouteAction::HostOffload
    );
    assert_eq!(
        policy.route("android.test.interface.IService_501", 502),
        RouteAction::LocalGuest
    );

    // Serde roundtrip on large rule table
    let serialized = serde_json::to_string(&policy).expect("Large table serialization failed");
    let deserialized: RoutingPolicy =
        serde_json::from_str(&serialized).expect("Large table deserialization failed");
    assert_eq!(deserialized.len(), 1000);
    assert_eq!(
        deserialized.route("android.test.interface.IService_998", 0),
        RouteAction::HostOffload
    );

    // Clear table
    policy.clear();
    assert_eq!(policy.len(), 0);
    assert!(policy.is_empty());
    assert_eq!(
        policy.route("android.test.interface.IService_500", 1),
        RouteAction::LocalGuest
    );
}
