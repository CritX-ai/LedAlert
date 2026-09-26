fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let sources = [
        "src/taskbar/macos_native.m",
        "src/displays/macos_native.m",
        "src/desktop/macos_native.m",
    ];
    for source in sources {
        println!("cargo::rerun-if-changed={source}");
    }
    cc::Build::new()
        .files(sources)
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .compile("ledalert_macos");
    for framework in [
        "AppKit",
        "Foundation",
        "CoreFoundation",
        "CoreGraphics",
        "ColorSync",
        "CoreAudio",
    ] {
        println!("cargo::rustc-link-lib=framework={framework}");
    }
    println!("cargo::rustc-link-lib=sqlite3");
}
