fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        let profile = std::env::var("PROFILE").unwrap_or_default();
        let exec_level = if profile == "release" {
            "requireAdministrator"
        } else {
            "asInvoker"
        };
        let manifest = format!(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="{}" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#, exec_level);
        res.set_manifest(&manifest);
        if let Err(e) = res.compile() {
            eprintln!("Error compiling windows resources: {}", e);
        }
    }
}
