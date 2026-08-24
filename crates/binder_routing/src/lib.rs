//! # binder_routing
//!
//! Selective Binder IPC Routing Policy Engine for AndroidWebGPU.
//!
//! Provides configurable routing policy tables and pattern matchers defaulting to local
//! guest execution (default-deny/local), enabling surgical offloading of specific interfaces
//! (e.g. SurfaceFlinger) across the VM boundary to host WebGPU services.

pub mod matcher;
pub mod policy;

pub use matcher::{
    CodeFilter, DescriptorMatcher, MatchRule, MatcherEngine, ServiceNameMatcher,
};
pub use policy::{RouteAction, RoutingPolicy, RoutingRule};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_local_deny() {
        let policy = RoutingPolicy::new_default_local();
        assert_eq!(
            policy.route("android.os.IBatteryStats", 1),
            RouteAction::LocalGuest
        );
        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1006),
            RouteAction::LocalGuest
        );
    }

    #[test]
    fn test_allow_host_offload() {
        let mut policy = RoutingPolicy::new_default_local();
        policy.allow_host_offload("android.gui.ISurfaceComposer");

        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1006),
            RouteAction::HostOffload
        );
        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1020),
            RouteAction::HostOffload
        );
        assert_eq!(
            policy.route("android.os.IPowerManager", 1),
            RouteAction::LocalGuest
        );
    }

    #[test]
    fn test_hybrid_opcode_routing() {
        let mut policy = RoutingPolicy::new_default_local();
        policy.allow_hybrid("android.gui.ISurfaceComposer", vec![1006, 1020]);

        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1006),
            RouteAction::HostOffload
        );
        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1020),
            RouteAction::HostOffload
        );
        assert_eq!(
            policy.route("android.gui.ISurfaceComposer", 1010),
            RouteAction::LocalGuest
        );
    }

    #[test]
    fn test_matcher_engine_patterns() {
        let mut engine = MatcherEngine::new(RouteAction::LocalGuest);

        // Offload all android.gui.* except code 99
        let rule = MatchRule::new(
            DescriptorMatcher::Prefix("android.gui.".to_string()),
            RouteAction::HostOffload,
        )
        .with_code_filter(CodeFilter::Except(vec![99]))
        .with_priority(10);
        engine.add_rule(rule);

        assert_eq!(
            engine.match_transaction(None, Some("android.gui.ISurfaceComposer"), 1006),
            RouteAction::HostOffload
        );
        assert_eq!(
            engine.match_transaction(None, Some("android.gui.IGraphicBufferProducer"), 1),
            RouteAction::HostOffload
        );
        assert_eq!(
            engine.match_transaction(None, Some("android.gui.ISurfaceComposer"), 99),
            RouteAction::LocalGuest
        );
        assert_eq!(
            engine.match_transaction(None, Some("android.os.IServiceManager"), 1),
            RouteAction::LocalGuest
        );
    }
}
