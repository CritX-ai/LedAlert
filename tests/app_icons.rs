use std::{
    fs,
    path::{Path, PathBuf},
};

use ledalert::app_icons::IconResolver;

struct Fixture {
    root: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let roots = vec![
            root.path().join("user/icons"),
            root.path().join("system/icons"),
        ];
        Self { root, roots }
    }

    fn write(&self, base: usize, path: &str, text: &str) {
        let path = self.roots[base].join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn theme(&self, base: usize, name: &str, inherits: &str) {
        self.write(
            base,
            &format!("{name}/index.theme"),
            &format!(
                "[Icon Theme]\nDirectories=16/apps,64/apps\nInherits={inherits}\n\
             [16/apps]\nSize=16\nType=Fixed\n[64/apps]\nSize=64\nType=Fixed\n"
            ),
        );
    }

    fn icon(&self, base: usize, path: &str, color: &str) {
        self.write(
            base,
            path,
            &svg(&format!("<rect width='64' height='64' fill='{color}'/>")),
        );
    }

    fn resolver(&self, theme: &str) -> IconResolver {
        IconResolver::new(
            theme,
            self.roots.clone(),
            vec![self.root.path().join("pixmaps")],
        )
    }
}

fn svg(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{body}</svg>")
}

fn png(path: &Path, width: u32, pixels: &[[u8; 4]]) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, pixels.len() as u32 / width);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels.as_flattened())
        .unwrap();
}

#[test]
fn theme_size_root_and_asset_precedence_do_not_change_application_identity() {
    let fixture = Fixture::new();
    fixture.theme(1, "Chosen", "Parent");
    fixture.theme(1, "Parent", "");
    fixture.icon(0, "Chosen/16/apps/app.svg", "#ff0000");
    fixture.icon(1, "Chosen/64/apps/app.svg", "#00ff00");
    fixture.icon(1, "Parent/64/apps/app.svg", "#0000ff");
    assert_eq!(
        fixture.resolver("Chosen").resolve("app").unwrap().dominant,
        [0, 255, 0]
    );
    fixture.icon(0, "Chosen/64/apps/app.svg", "#ff0000");
    assert_eq!(
        fixture.resolver("Chosen").resolve("app").unwrap().dominant,
        [255, 0, 0]
    );
    // An exact local PNG wins over SVG in the same theme directory.
    png(
        &fixture.roots[0].join("Chosen/64/apps/app.png"),
        1,
        &[[0, 0, 255, 255]],
    );
    assert_eq!(
        fixture.resolver("Chosen").resolve("app").unwrap().dominant,
        [0, 0, 255]
    );
    fs::write(fixture.roots[0].join("Chosen/64/apps/app.png"), b"invalid").unwrap();
    assert!(fixture.resolver("Chosen").resolve("app").is_none());
    // Current-theme artwork wins even when an inherited size would be closer.
    fixture.icon(0, "Chosen/16/apps/small.svg", "#ff0000");
    fixture.icon(1, "Parent/64/apps/small.svg", "#0000ff");
    assert_eq!(
        fixture
            .resolver("Chosen")
            .resolve("small")
            .unwrap()
            .dominant,
        [255, 0, 0]
    );
}

#[test]
fn inherited_themes_cycle_safely_before_hicolor_and_exact_unthemed_fallbacks() {
    let fixture = Fixture::new();
    fixture.theme(1, "Chosen", "hicolor,Parent");
    fixture.theme(1, "Parent", "Chosen,Grandparent");
    fixture.theme(1, "Grandparent", "");
    fixture.theme(1, "hicolor", "");
    fixture.icon(1, "Grandparent/64/apps/app.svg", "#00ff00");
    fixture.icon(1, "hicolor/64/apps/app.svg", "#ff0000");
    fixture.icon(1, "hicolor/64/apps/third-party.svg", "#0000ff");
    fixture.icon(0, "plain.svg", "#aabbcc");
    let pixmaps = fixture.root.path().join("pixmaps");
    fs::create_dir_all(&pixmaps).unwrap();
    fs::write(
        pixmaps.join("legacy.svg"),
        svg("<rect width='64' height='64' fill='#abcdef'/>"),
    )
    .unwrap();
    let mut resolver = fixture.resolver("Chosen");
    assert_eq!(resolver.resolve("app").unwrap().dominant, [0, 255, 0]);
    assert_eq!(
        resolver.resolve("third-party").unwrap().dominant,
        [0, 0, 255]
    );
    assert_eq!(
        resolver.resolve("plain.svg").unwrap().dominant,
        [170, 187, 204]
    );
    assert_eq!(
        resolver.resolve("legacy").unwrap().dominant,
        [171, 205, 239]
    );
    assert!(
        resolver.resolve("app-not-installed").is_none(),
        "no dash truncation or generic fallback"
    );
    assert!(resolver.resolve("../plain").is_none());
    assert!(
        resolver
            .resolve("https://example.invalid/plain.svg")
            .is_none()
    );
    assert!(resolver.resolve("file:///tmp/plain.svg").is_none());
}

#[test]
fn first_theme_index_controls_directories_and_scaled_entries_remain_eligible() {
    let fixture = Fixture::new();
    fixture.theme(1, "Chosen", "");
    fixture.write(0, "Chosen/index.theme", "[Icon Theme]\nDirectories=bad/../../escape\nScaledDirectories=32@2/apps\n[32@2/apps]\nSize=32\nScale=2\nType=Fixed\n");
    fixture.icon(1, "Chosen/64/apps/app.svg", "#ff0000");
    fixture.icon(1, "Chosen/32@2/apps/app.svg", "#00ff00");
    assert_eq!(
        fixture.resolver("Chosen").resolve("app").unwrap().dominant,
        [0, 255, 0]
    );
}

#[test]
fn dominant_color_uses_visible_chromatic_coverage_without_inventing_monochrome_hues() {
    let fixture = Fixture::new();
    let path = fixture.root.path().join("color.png");
    let mut pixels = vec![[255, 0, 0, 0]; 100];
    pixels.extend([[255, 0, 0, 8]; 100]);
    pixels.extend([[255, 255, 255, 255]; 60]);
    pixels.extend([[32, 176, 80, 255]; 30]);
    pixels.extend([[32, 80, 240, 255]; 10]);
    png(&path, 20, &pixels);
    let mut resolver = fixture.resolver("Absent");
    assert_eq!(
        resolver.resolve(path.to_str().unwrap()).unwrap().dominant,
        [32, 176, 80]
    );
    let mut small_mark = vec![[255, 255, 255, 255]; 99];
    small_mark.push([32, 176, 80, 255]);
    png(&path, 10, &small_mark);
    assert_eq!(
        resolver.resolve(path.to_str().unwrap()).unwrap().dominant,
        [32, 176, 80]
    );
    // Coverage is alpha-weighted, not an unweighted pixel count.
    let weighted = [[0, 255, 0, 32], [0, 255, 0, 32], [0, 0, 255, 255]];
    png(&path, 3, &weighted);
    assert_eq!(
        resolver.resolve(path.to_str().unwrap()).unwrap().dominant,
        [0, 0, 255]
    );
    png(
        &path,
        4,
        &[
            [0, 0, 0, 255],
            [192, 192, 192, 255],
            [192, 192, 192, 255],
            [192, 192, 192, 255],
        ],
    );
    assert_eq!(
        resolver.resolve(path.to_str().unwrap()).unwrap().dominant,
        [192; 3]
    );
    // Nearby reds spanning the circular hue boundary belong to one cluster.
    png(
        &path,
        3,
        &[[255, 0, 4, 255], [255, 4, 0, 255], [0, 0, 255, 255]],
    );
    assert_eq!(
        resolver.resolve(path.to_str().unwrap()).unwrap().dominant,
        [255, 2, 2]
    );
    png(&path, 1, &[[255, 0, 0, 0]]);
    assert!(resolver.resolve(path.to_str().unwrap()).is_none());
}

#[test]
fn svg_internal_gradients_work_but_executable_and_external_resources_are_rejected() {
    let fixture = Fixture::new();
    let path = fixture.root.path().join("icon.svg");
    let mut resolver = fixture.resolver("Absent");
    fs::write(&path, svg("<defs><linearGradient id='g'><stop stop-color='#00ff00'/><stop offset='1' stop-color='#00ff00'/></linearGradient></defs><rect width='64' height='64' fill='url(#g)'/>" )).unwrap();
    let safe = resolver.resolve(path.to_str().unwrap()).unwrap();
    assert_eq!(safe.dominant, [0, 255, 0]);
    assert_eq!(&safe.rgba[..4], &[0, 255, 0, 255]);
    for body in [
        "<image href='/tmp/private.png' width='64' height='64'/>",
        "<image href='https://example.invalid/icon.png'/>",
        "<image href='data:image/svg+xml,%3Csvg%3E'/>",
        "<use href='other.svg#shape'/>",
        "<use xmlns:xlink='http://www.w3.org/1999/xlink' xlink:href='file:///tmp/other.svg#shape'/>",
        "<rect width='64' height='64' fill='url(https://example.invalid/paint)'/>",
        "<style>@import 'https://example.invalid/style.css';</style>",
        "<style>rect { fill: u\\72l(https://example.invalid/paint); }</style>",
        "<rect width='64' height='64' style='fill:url(&#x68;ttps://example.invalid/paint)'/>",
        "<script>alert(1)</script>",
        "<rect width='64' height='64' onload='alert(1)'/>",
        "<foreignObject width='64' height='64'/>",
        "<?xml-stylesheet href='https://example.invalid/style.css'?>",
    ] {
        fs::write(
            &path,
            svg(&format!("<rect width='64' height='64' fill='red'/>{body}")),
        )
        .unwrap();
        assert!(
            resolver.resolve(path.to_str().unwrap()).is_none(),
            "unsafe body: {body}"
        );
    }
    fs::write(
        &path,
        format!(
            "<!DOCTYPE svg [<!ENTITY external SYSTEM 'file:///tmp/private'>]>{}",
            svg("<text>&external;</text>")
        ),
    )
    .unwrap();
    assert!(resolver.resolve(path.to_str().unwrap()).is_none());
    fs::write(
        &path,
        format!(
            "<!DOCTYPE svg SYSTEM 'https://example.invalid/svg.dtd'>{}",
            svg("<rect width='64' height='64' fill='red'/>")
        ),
    )
    .unwrap();
    assert!(resolver.resolve(path.to_str().unwrap()).is_none());
}

#[test]
fn rasterization_preserves_unpremultiplied_color_and_caps_retained_dimensions() {
    let fixture = Fixture::new();
    let path = fixture.root.path().join("icon.svg");
    fs::write(
        &path,
        svg("<rect width='64' height='64' fill='#0080ff' opacity='0.5'/>"),
    )
    .unwrap();
    let mut resolver = fixture.resolver("Absent");
    let icon = resolver.resolve(path.to_str().unwrap()).unwrap();
    assert_eq!(icon.dominant, [0, 128, 255]);
    assert_eq!(&icon.rgba[..4], &[0, 128, 255, 128]);
    let path = fixture.root.path().join("large.png");
    png(&path, 128, &vec![[32, 160, 64, 255]; 128 * 256]);
    let icon = resolver.resolve(path.to_str().unwrap()).unwrap();
    assert_eq!((icon.width, icon.height), (32, 64));
    assert_eq!(icon.dominant, [32, 160, 64]);
    assert_eq!(&icon.rgba[(16 * 32 + 16) * 4..][..4], &[32, 160, 64, 255]);
    png(&path, 1025, &vec![[32, 160, 64, 255]; 1025]);
    assert!(resolver.resolve(path.to_str().unwrap()).is_none());
    let oversized = fixture.root.path().join("oversized.svg");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    assert!(resolver.resolve(oversized.to_str().unwrap()).is_none());
}

#[test]
fn full_sized_hicolor_indexes_remain_searchable_near_the_end() {
    let fixture = Fixture::new();
    // Real hicolor distributions contain over 600 directory sections. The
    // metadata bound must not discard that standard fallback as an oversized theme.
    let directories = (0..650)
        .map(|index| format!("64/context-{index}"))
        .collect::<Vec<_>>();
    let mut index = format!("[Icon Theme]\nDirectories={}\n", directories.join(","));
    for directory in &directories {
        index.push_str(&format!("[{directory}]\nSize=64\nType=Fixed\n"));
    }
    fixture.write(1, "hicolor/index.theme", &index);
    fixture.icon(1, "hicolor/64/context-649/app.svg", "#20b050");
    assert_eq!(
        fixture.resolver("Absent").resolve("app").unwrap().dominant,
        [32, 176, 80]
    );
}
