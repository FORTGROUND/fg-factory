// No Windows, o ícone do FG Factory vai para o .exe (Explorer, barra de tarefas).
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "FG Factory Setup");
        res.set("FileDescription", "Configura os runners do FG Factory");
        res.compile().expect("ícone do Windows");
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
