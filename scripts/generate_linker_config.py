#!/usr/bin/env python3
"""Generate ld.config.txt and apex-info-list.xml for Android 14 flattened APEXes."""

APEX_PACKAGES = [
    "com.android.adbd",
    "com.android.adservices",
    "com.android.appsearch",
    "com.android.art",
    "com.android.btservices",
    "com.android.cellbroadcast",
    "com.android.compos",
    "com.android.configinfrastructure",
    "com.android.conscrypt",
    "com.android.devicelock",
    "com.android.extservices",
    "com.android.healthfitness",
    "com.android.i18n",
    "com.android.ipsec",
    "com.android.media",
    "com.android.media.swcodec",
    "com.android.mediaprovider",
    "com.android.neuralnetworks",
    "com.android.ondevicepersonalization",
    "com.android.os.statsd",
    "com.android.permission",
    "com.android.resolv",
    "com.android.rkpd",
    "com.android.runtime",
    "com.android.scheduling",
    "com.android.sdkext",
    "com.android.tethering",
    "com.android.tzdata",
    "com.android.uwb",
    "com.android.virt",
    "com.android.wifi",
]

def generate_ld_config() -> str:
    apex_ns_map = {pkg: pkg.replace(".", "_") for pkg in APEX_PACKAGES}
    all_apex_namespaces = list(apex_ns_map.values())
    
    # Core system namespaces required by ART and native loader
    # art: ART runtime namespace
    # system: OAT file loading namespace (art::GetSystemLinkerNamespace)
    # boot: Boot namespace
    # sphal: Same-process HAL
    # vndk: Vendor NDK
    special_namespaces = ["system", "art", "boot", "sphal", "vndk"]
    all_target_namespaces = special_namespaces + all_apex_namespaces

    # Default search paths must include system, vendor, AND all APEX library directories
    # so that dlopen("libart.so") or dlopen from any un-isolated namespace succeeds directly.
    default_search = [
        "/system/${LIB}",
        "/system/lib64",
        "/system/lib",
        "/system_ext/${LIB}",
        "/system_ext/lib64",
        "/product/${LIB}",
        "/product/lib64",
        "/vendor/${LIB}",
        "/vendor/lib64",
    ]
    for pkg in APEX_PACKAGES:
        default_search.append(f"/apex/{pkg}/${{LIB}}")
        default_search.append(f"/apex/{pkg}/lib64")
        default_search.append(f"/apex/{pkg}/lib")

    search_paths_str = ":".join(default_search)

    lines = [
        "# Linker configuration generated for Android 14 with flattened APEX namespaces",
        "dir.system = /system/bin",
        "dir.system = /system_ext/bin",
        "dir.system = /system",
        "dir.system = /product/bin",
        "dir.system = /product",
        "dir.system = /vendor/bin",
        "dir.system = /vendor",
        "dir.system = /data",
        "dir.system = /apex",
        "dir.system = /",
        "",
        "[system]",
        f"additional.namespaces = {','.join(all_target_namespaces)}",
        "",
        "###############################################################################",
        "# 'default' namespace",
        "###############################################################################",
        "namespace.default.isolated = false",
        "namespace.default.visible = true",
        f"namespace.default.search.paths = {search_paths_str}",
        "namespace.default.permitted.paths = /system:/system_ext:/product:/vendor:/apex:/data",
        f"namespace.default.links = {','.join(all_target_namespaces)}",
    ]

    for ns in all_target_namespaces:
        lines.append(f"namespace.default.link.{ns}.allow_all_shared_libs = true")

    lines.extend([
        "",
        "###############################################################################",
        "# 'system' namespace (queried by art::GetSystemLinkerNamespace for OAT loading)",
        "###############################################################################",
        "namespace.system.isolated = false",
        "namespace.system.visible = true",
        f"namespace.system.search.paths = {search_paths_str}",
        "namespace.system.permitted.paths = /system:/system_ext:/product:/vendor:/apex:/data",
        "namespace.system.links = default",
        "namespace.system.link.default.allow_all_shared_libs = true",
        "",
        "###############################################################################",
        "# 'boot' namespace",
        "###############################################################################",
        "namespace.boot.isolated = false",
        "namespace.boot.visible = true",
        f"namespace.boot.search.paths = {search_paths_str}",
        "namespace.boot.permitted.paths = /system:/system_ext:/product:/vendor:/apex:/data",
        "namespace.boot.links = default",
        "namespace.boot.link.default.allow_all_shared_libs = true",
        "",
        "###############################################################################",
        "# 'art' namespace alias for com.android.art",
        "###############################################################################",
        "namespace.art.isolated = false",
        "namespace.art.visible = true",
        "namespace.art.search.paths = /apex/com.android.art/${LIB}:/apex/com.android.art/lib64:/apex/com.android.art/lib:/system/${LIB}:/system/lib64:/system/lib",
        "namespace.art.permitted.paths = /apex/com.android.art:/system:/system_ext:/product:/vendor:/data",
        "namespace.art.links = default",
        "namespace.art.link.default.allow_all_shared_libs = true",
        "",
        "###############################################################################",
        "# 'sphal' namespace",
        "###############################################################################",
        "namespace.sphal.isolated = false",
        "namespace.sphal.visible = true",
        "namespace.sphal.search.paths = /vendor/${LIB}:/vendor/${LIB}/hw:/system/${LIB}",
        "namespace.sphal.permitted.paths = /vendor:/system",
        "namespace.sphal.links = default",
        "namespace.sphal.link.default.allow_all_shared_libs = true",
        "",
        "###############################################################################",
        "# 'vndk' namespace",
        "###############################################################################",
        "namespace.vndk.isolated = false",
        "namespace.vndk.visible = true",
        "namespace.vndk.search.paths = /system/${LIB}/vndk:/system/${LIB}:/vendor/${LIB}",
        "namespace.vndk.permitted.paths = /system:/vendor",
        "namespace.vndk.links = default",
        "namespace.vndk.link.default.allow_all_shared_libs = true",
        "",
    ])

    for pkg, ns in apex_ns_map.items():
        lines.extend([
            "###############################################################################",
            f"# '{ns}' namespace for {pkg}",
            "###############################################################################",
            f"namespace.{ns}.isolated = false",
            f"namespace.{ns}.visible = true",
            f"namespace.{ns}.search.paths = /apex/{pkg}/${{LIB}}:/apex/{pkg}/lib64:/apex/{pkg}/lib:/system/${{LIB}}:/system/lib64:/system/lib",
            f"namespace.{ns}.permitted.paths = /apex/{pkg}:/system:/system_ext:/product:/vendor:/data",
            f"namespace.{ns}.links = default",
            f"namespace.{ns}.link.default.allow_all_shared_libs = true",
            "",
        ])

    return "\n".join(lines) + "\n"

def generate_apex_info_list() -> str:
    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        '<apex-info-list>',
    ]
    for pkg in APEX_PACKAGES:
        lines.append(
            f'  <apex-info moduleName="{pkg}" modulePath="/apex/{pkg}" '
            f'preinstalledModulePath="/system/system_ext/apex/{pkg}" '
            f'versionCode="340000000" versionName="14.0" isFactory="true" '
            f'isActive="true" lastUpdateMillis="0" provideSharedApexLibs="false" />'
        )
    lines.append('</apex-info-list>\n')
    return "\n".join(lines)

if __name__ == "__main__":
    cfg = generate_ld_config()
    xml = generate_apex_info_list()
    print(f"Generated ld.config.txt: {len(cfg)} bytes")
    print(f"Generated apex-info-list.xml: {len(xml)} bytes")
    assert "com_android_tethering" in cfg
    assert "namespace.com_android_tethering.visible = true" in cfg
    assert "namespace.com_android_tethering.search.paths" in cfg
    assert "/apex/com.android.art/lib64" in cfg
    assert "namespace.system.visible = true" in cfg
    assert "namespace.art.visible = true" in cfg
    assert "com.android.tethering" in xml
    print("Self-test passed!")
