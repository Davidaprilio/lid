//! Windows: the program's icon and version information in lider.exe (the Explorer, the taskbar, the
//! installer's shortcuts).

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico")
            .set("ProductName", "LID")
            .set("FileDescription", "LID, Linux Internet Downloader")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no Windows resources: {e}");
        }
    }
}
