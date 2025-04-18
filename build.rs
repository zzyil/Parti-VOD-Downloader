fn main() {
    // Only embed the icon on Windows
    #[cfg(windows)]
    {
        embed_resource::compile("assets/windows.rc");
    }
}
