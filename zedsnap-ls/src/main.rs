fn main() -> anyhow::Result<()> {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("zedsnap-ls {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--themes" => {
                for theme in zedsnap_ls::highlight::available_themes() {
                    println!("{theme}");
                }
                return Ok(());
            }
            other => {
                eprintln!("zedsnap-ls: ignoring unknown argument {other:?}");
            }
        }
    }

    zedsnap_ls::server::run()
}
