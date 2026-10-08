use std::fs::File;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=assets/branding/macncheese-512.png");

    #[cfg(windows)]
    {
        // Build a multi-size .ico from branding PNGs so Explorer,
        // taskbar and Alt-Tab all show our logo.
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let ico_path = out_dir.join("app.ico");
        if let Err(e) = make_ico(&ico_path) {
            panic!("icon build failed: {e}");
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon(ico_path.to_string_lossy().as_ref());
        res.set("ProductName", "Cheesestrap");
        res.set(
            "FileDescription",
            "Cheesestrap - cheesy Roblox bootstrapper",
        );
        if let Err(e) = res.compile() {
            panic!("winres failed: {e}");
        }
        // GNU ld drops resource-only archive members, so link the
        // object directly instead of relying on libresource.a.
        // MSVC link.exe handles it fine, no hack needed there.
        if std::env::var("TARGET")
            .map(|t| t.contains("windows-gnu"))
            .unwrap_or(false)
        {
            println!(
                "cargo::rustc-link-arg={}/resource.o",
                out_dir.to_string_lossy()
            );
        }
    }
}

#[cfg(windows)]
fn make_ico(dest: &PathBuf) -> Result<(), String> {
    let src = image::open("assets/branding/macncheese-512.png")
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [256u32, 48, 32, 16] {
        let resized = image::imageops::resize(
            &src,
            size,
            size,
            image::imageops::FilterType::Lanczos3,
        );
        let entry = ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(
            size,
            size,
            resized.into_raw(),
        ))
        .map_err(|e| e.to_string())?;
        dir.add_entry(entry);
    }
    let file = File::create(dest).map_err(|e| e.to_string())?;
    dir.write(file).map_err(|e| e.to_string())?;
    Ok(())
}
