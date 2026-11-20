fn main() {
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/brand/icon/bronze-dark.ico")
        .compile()
        .expect("embedding the Windows icon needs the Windows SDK's resource compiler");
}
